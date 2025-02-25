use std::collections::VecDeque;
use std::{mem, thread::JoinHandle};

use crossbeam::channel::RecvTimeoutError;
use massa_channel::receiver::MassaReceiver;
use massa_logging::massa_trace;
use massa_metrics::MassaMetrics;
use massa_models::operation::OperationId;
use massa_models::prehash::CapacityAllocator;
use massa_models::prehash::PreHashSet;
use massa_protocol_exports::PeerId;
use massa_protocol_exports::ProtocolConfig;
use massa_protocol_exports::ProtocolError;
use massa_storage::Storage;
use tracing::{debug, info, log::warn};

use crate::{
    handlers::operation_handler::OperationMessage, messages::MessagesSerializer,
    wrap_network::ActiveConnectionsTrait,
};

use super::{
    cache::SharedOperationCache, commands_propagation::OperationHandlerPropagationCommand,
    OperationMessageSerializer,
};

// protocol-operation-handler-propagation
const THREAD_NAME: &str = "poh-tester";
static_assertions::const_assert!(THREAD_NAME.len() < 16);

struct PropagationThread {
    internal_receiver: MassaReceiver<OperationHandlerPropagationCommand>,
    active_connections: Box<dyn ActiveConnectionsTrait>,
    // times at which previous ops were announced
    stored_for_propagation: VecDeque<(std::time::Instant, PreHashSet<OperationId>)>,
    op_storage: Storage,
    next_batch: PreHashSet<OperationId>,
    config: ProtocolConfig,
    cache: SharedOperationCache,
    operation_message_serializer: MessagesSerializer,
    _massa_metrics: MassaMetrics,
}

impl PropagationThread {
    fn run(&mut self) {
        let mut batch_deadline = std::time::Instant::now()
            .checked_add(self.config.operation_announcement_interval.to_duration())
            .expect("Can't init interval op propagation");
        loop {
            match self.internal_receiver.recv_deadline(batch_deadline) {
                Ok(internal_message) => {
                    match internal_message {
                        OperationHandlerPropagationCommand::PropagateOperations(operations) => {
                            // Note operations as checked.
                            {
                                let mut cache_write = self.cache.write();
                                for op_id in operations.get_op_refs().iter().copied() {
                                    cache_write.insert_checked_operation(op_id);
                                }
                            }

                            // add to propagation storage
                            let new_ops = operations.get_op_refs().clone();
                            self.stored_for_propagation
                                .push_back((std::time::Instant::now(), new_ops.clone()));
                            self.op_storage.extend(operations);
                            self.prune_propagation_storage();

                            for op_id in new_ops {
                                self.next_batch.insert(op_id);
                                if self.next_batch.len()
                                    >= self.config.operation_announcement_buffer_capacity
                                {
                                    self.announce_ops();
                                    batch_deadline = std::time::Instant::now()
                                        .checked_add(
                                            self.config
                                                .operation_announcement_interval
                                                .to_duration(),
                                        )
                                        .expect("Can't init interval op propagation");
                                }
                            }
                        }
                        OperationHandlerPropagationCommand::Stop => {
                            info!("Stop operation propagation thread");
                            return;
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.announce_ops();
                    batch_deadline = std::time::Instant::now()
                        .checked_add(self.config.operation_announcement_interval.to_duration())
                        .expect("Can't init interval op propagation");
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return;
                }
            }
        }
    }

    /// Prune the list of operations kept for propagation.
    fn prune_propagation_storage(&mut self) {
        let mut removed = PreHashSet::default();

        // remove expired
        let max_op_prop_time = self.config.max_operations_propagation_time.to_duration();
        while let Some((t, _)) = self.stored_for_propagation.front() {
            if t.elapsed() > max_op_prop_time {
                let (_, op_ids) = self
                    .stored_for_propagation
                    .pop_front()
                    .expect("there should be at least one element, checked above");
                removed.extend(op_ids);
            } else {
                break;
            }
        }

        // Cap cache size
        // Note that we directly remove batches of operations, not individual operations
        // to favor simplicity and performance over precision.
        let mut excess_count = self
            .stored_for_propagation
            .iter()
            .map(|(_, ops)| ops.len())
            .sum::<usize>()
            .saturating_sub(self.config.max_ops_kept_for_propagation);
        while excess_count > 0 {
            if let Some((_t, op_ids)) = self.stored_for_propagation.pop_front() {
                excess_count = excess_count.saturating_sub(op_ids.len());
                removed.extend(op_ids);
            } else {
                break;
            }
        }

        // remove from storage
        self.op_storage.drop_operation_refs(&removed);
    }

    fn announce_ops(&mut self) {
        // Quit if empty  to avoid iterating on nodes
        if self.next_batch.is_empty() {
            return;
        }
        let operation_ids = mem::take(&mut self.next_batch);
        massa_trace!("protocol.protocol_worker.announce_ops.begin", {
            "operation_ids": operation_ids
        });
        {
            let mut cache_write = self.cache.write();
            let peers_connected = self.active_connections.get_peer_ids_connected();
            cache_write.update_cache(&peers_connected);

            // Propagate to peers
            let all_keys: Vec<PeerId> = cache_write.ops_known_by_peer.keys().cloned().collect();
            for peer_id in all_keys {
                let ops = cache_write.ops_known_by_peer.get_mut(&peer_id).unwrap();
                let new_ops: Vec<OperationId> = operation_ids
                    .iter()
                    .filter(|id| ops.peek(&id.prefix()).is_none())
                    .copied()
                    .collect();
                if !new_ops.is_empty() {
                    for id in &new_ops {
                        ops.insert(id.prefix(), ());
                    }
                    debug!(
                        "Send operations announcement of len {} to {}",
                        new_ops.len(),
                        peer_id
                    );
                    for sub_list in new_ops.chunks(self.config.max_operations_per_message as usize)
                    {
                        if let Err(err) = self.active_connections.send_to_peer(
                            &peer_id,
                            &self.operation_message_serializer,
                            OperationMessage::OperationsAnnouncement(
                                sub_list.iter().map(|id| id.into_prefix()).collect(),
                            )
                            .into(),
                            false,
                        ) {
                            warn!(
                                "Failed to send OperationsAnnouncement message to peer: {}",
                                err
                            );

                            if let ProtocolError::PeerDisconnected(_) = err {
                                // cache of this peer is removed in next call of cache_write.update_cache
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn start_propagation_thread(
    internal_receiver: MassaReceiver<OperationHandlerPropagationCommand>,
    active_connections: Box<dyn ActiveConnectionsTrait>,
    config: ProtocolConfig,
    cache: SharedOperationCache,
    op_storage: Storage,
    massa_metrics: MassaMetrics,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name(THREAD_NAME.to_string())
        .spawn(move || {
            let mut propagation_thread = PropagationThread {
                internal_receiver,
                active_connections,
                stored_for_propagation: VecDeque::with_capacity(
                    config.max_ops_kept_for_propagation,
                ),
                op_storage,
                next_batch: PreHashSet::with_capacity(
                    config
                        .operation_announcement_buffer_capacity
                        .saturating_add(1),
                ),
                config,
                cache,
                _massa_metrics: massa_metrics,
                operation_message_serializer: MessagesSerializer::new()
                    .with_operation_message_serializer(OperationMessageSerializer::new()),
            };
            propagation_thread.run();
        })
        .expect("OS failed to start operation propagation thread")
}
