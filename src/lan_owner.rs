//! Application-thread LAN protocol state. No network task owns these documents.
use crate::{
    commands::{LanPeerStatus, LanStatus},
    lan::{self, Body, Control, Controller, DocumentId, Event, Fingerprint, Packet, Request},
    sync::{DocumentHistory, HistoryBundle, Replica},
};
use automerge::{
    AutoCommit, LoadOptions, TextEncoding,
    sync::{Message, State, SyncDoc},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::mpsc,
};
use uuid::Uuid;
const CHUNK: usize = 64 * 1024;
const JOIN_LIMIT: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preparation {
    pub vault_id: Uuid,
    pub device_id: Uuid,
    pub ticket: String,
}
pub(crate) struct Context<'a> {
    pub history: Option<&'a HistoryBundle>,
    pub accepted: u64,
    pub persisted: u64,
}
pub(crate) enum Action {
    Status(Uuid, Result<(), String>),
    Receive {
        peer: Fingerprint,
        bundle: HistoryBundle,
        receipts: Vec<Uuid>,
    },
    Join {
        peer: Fingerprint,
        session: usize,
        transfer: Uuid,
        bundle: HistoryBundle,
    },
}
struct Documents {
    vault: Uuid,
    device: Uuid,
    docs: HashMap<DocumentId, AutoCommit>,
}
impl Documents {
    fn load(bundle: &HistoryBundle) -> Result<Self, String> {
        let mut docs = HashMap::new();
        docs.insert(DocumentId::Catalog, load(&bundle.catalog)?);
        for doc in &bundle.documents {
            docs.insert(DocumentId::Note(doc.id), load(&doc.bytes)?);
        }
        Ok(Self {
            vault: bundle.vault_id,
            device: bundle.device_id,
            docs,
        })
    }
    fn bundle(&mut self) -> HistoryBundle {
        let catalog = self.docs.get_mut(&DocumentId::Catalog).unwrap().save();
        let documents = self
            .docs
            .iter_mut()
            .filter_map(|(id, doc)| match id {
                DocumentId::Note(id) => Some(DocumentHistory {
                    id: *id,
                    bytes: doc.save(),
                }),
                _ => None,
            })
            .collect();
        HistoryBundle {
            version: 1,
            vault_id: self.vault,
            device_id: self.device,
            catalog,
            documents,
        }
    }
    fn receive(&mut self, id: DocumentId, bytes: &[u8], state: &mut State) -> Result<bool, String> {
        if self.docs.len() > 10_001 && !self.docs.contains_key(&id) {
            return Err("document count limit".into());
        }
        let doc = self
            .docs
            .entry(id)
            .or_insert_with(|| AutoCommit::new_with_encoding(TextEncoding::UnicodeCodePoint));
        let before = doc.get_heads();
        doc.sync()
            .receive_sync_message(state, Message::decode(bytes).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(before != doc.get_heads())
    }
}
fn load(bytes: &[u8]) -> Result<AutoCommit, String> {
    AutoCommit::load_with_options(
        bytes,
        LoadOptions::new().text_encoding(TextEncoding::UnicodeCodePoint),
    )
    .map_err(|e| e.to_string())
}
struct OutJoin {
    transfer: Uuid,
    bytes: Vec<u8>,
    digest: Vec<u8>,
    next: usize,
}
struct InJoin {
    peer: Fingerprint,
    session: usize,
    transfer: Uuid,
    count: usize,
    total: usize,
    digest: Vec<u8>,
    next: usize,
    bytes: Vec<u8>,
}
struct Peer {
    session: usize,
    states: HashMap<DocumentId, State>,
    docs: Option<Documents>,
    view_generation: u64,
    blocked: Option<u64>,
    dirty: bool,
    changed: HashSet<DocumentId>,
    receipts: Vec<Uuid>,
    last_receipt: Option<Uuid>,
    out_join: Option<OutJoin>,
    join_wait: Option<Uuid>,
    cursor: usize,
    manifest_generation: Option<u64>,
}
impl Peer {
    fn new(session: usize) -> Self {
        Self {
            session,
            states: HashMap::new(),
            docs: None,
            view_generation: u64::MAX,
            blocked: None,
            dirty: false,
            changed: HashSet::new(),
            receipts: Vec::new(),
            last_receipt: None,
            out_join: None,
            join_wait: None,
            cursor: 0,
            manifest_generation: None,
        }
    }
}
#[derive(Default)]
pub(crate) struct LanState {
    pub runtime: Option<tokio::runtime::Handle>,
    pub preparation: Option<Preparation>,
    pub controller: Option<Controller>,
    starting: Option<(Uuid, mpsc::Receiver<Result<Controller, String>>)>,
    config: Option<(SocketAddr, Option<IpAddr>)>,
    peers: HashMap<Fingerprint, Peer>,
    outbound: VecDeque<(Fingerprint, Packet)>,
    incoming: Option<InJoin>,
    controls: HashMap<Uuid, Option<Fingerprint>>,
    cancelled: Vec<Uuid>,
    denied: HashSet<Fingerprint>,
    sending: HashMap<Fingerprint, (usize, Uuid)>,
    pub committing: bool,
    pub paused: bool,
    pub error: Option<String>,
}
impl LanState {
    pub fn start(
        &mut self,
        request: Uuid,
        directory: PathBuf,
        vault: Uuid,
        bind: SocketAddr,
        advertised: Option<IpAddr>,
    ) -> Result<(), String> {
        if self.starting.is_some() {
            return Err("listener startup is pending".into());
        }
        let runtime = self
            .runtime
            .as_ref()
            .ok_or("LAN I/O runtime not installed")?;
        self.cancelled
            .extend(self.controls.drain().map(|(id, _)| id));
        self.sending.clear();
        self.controller.take();
        self.peers.clear();
        self.outbound.clear();
        let (tx, rx) = mpsc::channel();
        runtime.spawn(async move {
            let result = Controller::bind(&directory, vault, bind)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        self.starting = Some((request, rx));
        self.config = Some((bind, advertised));
        self.paused = false;
        Ok(())
    }
    pub fn resume(&mut self, request: Uuid, directory: PathBuf, vault: Uuid) -> Result<(), String> {
        let (bind, ip) = self.config.ok_or("no prior listener configuration")?;
        self.start(request, directory, vault, bind, ip)
    }
    pub fn with_preparation(preparation: Option<Preparation>) -> Self {
        Self {
            preparation,
            ..Self::default()
        }
    }
    pub fn pause(&mut self) -> Option<Uuid> {
        // Dropping the receiver also drops any late controller sent by startup.
        let cancelled = self.starting.take().map(|(id, _)| id);
        self.cancelled
            .extend(self.controls.drain().map(|(id, _)| id));
        self.sending.clear();
        self.controller.take();
        self.peers.clear();
        self.outbound.clear();
        self.incoming = None;
        self.paused = true;
        cancelled
    }
    pub fn ticket(&self) -> Result<String, String> {
        let controller = self.controller.as_ref().ok_or("LAN listener unavailable")?;
        let advertised = self.config.and_then(|(_, ip)| {
            ip.map(|ip| SocketAddr::new(ip, controller.local_address().port()))
        });
        controller
            .ticket(advertised)
            .map_err(|e| e.to_string())?
            .encode()
            .map_err(|e| e.to_string())
    }
    pub fn control(&mut self, id: Uuid, operation: Control) -> Result<(), String> {
        let confirmation = match &operation {
            Control::Confirm(ticket) => Some(ticket.fingerprint.clone()),
            _ => None,
        };
        self.controller
            .as_ref()
            .ok_or("LAN listener unavailable")?
            .try_send(Request::Control {
                request_id: id,
                operation,
            })
            .map_err(|_| String::from("LAN command queue full"))?;
        self.controls.insert(id, confirmation);
        Ok(())
    }
    pub fn known(&self, peer: &str) -> Result<lan::Ticket, String> {
        self.controller
            .as_ref()
            .ok_or("LAN listener unavailable")?
            .confirmed_peers()
            .into_iter()
            .find(|ticket| ticket.fingerprint.0 == peer)
            .ok_or("peer is not confirmed".into())
    }
    pub fn deny(&mut self, peer: &Fingerprint) {
        self.denied.insert(peer.clone());
        let obsolete: Vec<_> = self
            .controls
            .iter()
            .filter_map(|(id, confirmation)| (confirmation.as_ref() == Some(peer)).then_some(*id))
            .collect();
        for id in obsolete {
            self.controls.remove(&id);
            self.cancelled.push(id);
        }
        self.revoke(peer);
    }
    pub fn revoke(&mut self, peer: &Fingerprint) {
        self.sending.remove(peer);
        self.peers.remove(peer);
        self.outbound.retain(|(id, _)| id != peer);
        if self
            .incoming
            .as_ref()
            .is_some_and(|join| &join.peer == peer)
        {
            self.incoming = None;
        }
    }
    fn enqueue(
        &mut self,
        peer: Fingerprint,
        document: DocumentId,
        body: Body,
        vault: Uuid,
    ) -> Result<(), String> {
        if self.outbound.len() >= 64 {
            return Err("owner LAN queue is full".into());
        }
        let packet = Packet {
            version: 1,
            vault_id: vault,
            document,
            message_id: Uuid::new_v4(),
            body,
        };
        packet.validate(vault).map_err(|e| e.to_string())?;
        self.outbound.push_back((peer, packet));
        Ok(())
    }
    pub(crate) fn fail(&mut self, peer: &Fingerprint, error: String) {
        self.error = Some(error);
        if let Some(controller) = &self.controller {
            let _ = controller.try_send(Request::Disconnect(peer.clone()));
        }
        self.revoke(peer);
    }
    fn finish_send(&mut self, peer: &Fingerprint, session: usize, message_id: Uuid) -> bool {
        if self
            .peers
            .get(peer)
            .is_some_and(|state| state.session == session)
            && self.sending.get(peer) == Some(&(session, message_id))
        {
            self.sending.remove(peer);
            true
        } else {
            false
        }
    }
    pub fn received(&mut self, peer: &Fingerprint, generation: u64, receipts: Vec<Uuid>) {
        if let Some(state) = self.peers.get_mut(peer) {
            state.blocked = Some(generation);
            state.dirty = false;
            state.receipts.extend(receipts);
        }
    }
    pub fn joined(
        &mut self,
        peer: &Fingerprint,
        session: usize,
        transfer: Uuid,
        bundle: &HistoryBundle,
    ) -> Result<(), String> {
        self.committing = false;
        self.preparation = None;
        if self
            .peers
            .get(peer)
            .is_some_and(|state| state.session == session)
        {
            self.enqueue(
                peer.clone(),
                DocumentId::Catalog,
                Body::DurableReceipt { received: transfer },
                bundle.vault_id,
            )?;
            if let Some(state) = self.peers.get_mut(peer) {
                state.states.clear();
                state.docs = None;
                state.view_generation = u64::MAX;
            }
        }
        Ok(())
    }
    pub fn status(&self, context: &Context<'_>) -> LanStatus {
        let mut peers: Vec<_> = self
            .controller
            .as_ref()
            .map(|controller| {
                controller
                    .confirmed_peers()
                    .into_iter()
                    .map(|ticket| {
                        let state = self.peers.get(&ticket.fingerprint);
                        LanPeerStatus {
                            fingerprint: ticket.fingerprint.0,
                            connected: state.is_some(),
                            session: state.map(|state| state.session as u64),
                            last_receipt: state.and_then(|state| state.last_receipt),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        peers.sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));
        LanStatus {
            listening: self.controller.is_some(),
            paused: self.paused,
            prepared: self.preparation.is_some(),
            joining: self.incoming.is_some() || self.committing,
            vault_id: context
                .history
                .map(|h| h.vault_id)
                .or_else(|| self.preparation.as_ref().map(|p| p.vault_id)),
            ticket: self.ticket().ok(),
            peers,
            pending_messages: self.outbound.len()
                + self
                    .peers
                    .values()
                    .map(|peer| {
                        peer.receipts.len()
                            + usize::from(peer.out_join.is_some() || peer.join_wait.is_some())
                    })
                    .sum::<usize>(),
            error: self.error.clone(),
            accepted_generation: context.accepted,
            persisted_generation: context.persisted,
        }
    }
    pub fn tick(&mut self, context: &Context<'_>) -> Vec<Action> {
        let mut actions: Vec<_> = self
            .cancelled
            .drain(..)
            .map(|id| {
                Action::Status(
                    id,
                    Err("LAN controller retired; operation outcome unknown".into()),
                )
            })
            .collect();
        // Do not consume further protocol input until the previous receive is durable.
        if self.peers.values().any(|peer| {
            peer.blocked
                .is_some_and(|generation| generation > context.persisted)
        }) {
            return actions;
        }
        for state in self.peers.values_mut() {
            if state.blocked.take().is_some() {
                state.docs = None;
                state.changed.clear();
            }
        }
        if let Some((id, rx)) = &self.starting {
            if let Ok(result) = rx.try_recv() {
                let id = *id;
                self.starting = None;
                match result {
                    Ok(controller) => {
                        self.controller = Some(controller);
                        self.error = None;
                        actions.push(Action::Status(id, Ok(())));
                    }
                    Err(error) => {
                        self.error = Some(error.clone());
                        actions.push(Action::Status(id, Err(error)));
                    }
                }
            }
        }
        for _ in 0..16 {
            let Some(event) = self.controller.as_mut().and_then(Controller::try_recv) else {
                break;
            };
            match event {
                Event::ControlResult { request_id, result } => {
                    if let Some(confirmation) = self.controls.remove(&request_id) {
                        if result.is_ok() {
                            if let Some(peer) = confirmation {
                                self.denied.remove(&peer);
                            }
                        }
                        actions.push(Action::Status(request_id, result));
                    }
                }
                Event::Confirmed(_) => {}
                Event::Unpaired(peer) => self.revoke(&peer),
                Event::Connected { peer, session } => {
                    if self.denied.contains(&peer) {
                        if let Some(controller) = &self.controller {
                            let _ = controller.try_send(Request::Disconnect(peer));
                        }
                        continue;
                    }
                    if !self
                        .controller
                        .as_ref()
                        .is_some_and(|controller| controller.is_confirmed(&peer))
                    {
                        continue;
                    }
                    self.revoke(&peer);
                    self.peers.insert(peer.clone(), Peer::new(session));
                    if context.history.is_none() {
                        if let Some(preparation) = &self.preparation {
                            if lan::Ticket::parse(&preparation.ticket)
                                .is_ok_and(|ticket| ticket.fingerprint == peer)
                            {
                                let transfer = Uuid::new_v4();
                                self.incoming = Some(InJoin {
                                    peer: peer.clone(),
                                    session,
                                    transfer,
                                    count: 0,
                                    total: 0,
                                    digest: Vec::new(),
                                    next: 0,
                                    bytes: Vec::new(),
                                });
                                let _ = self.enqueue(
                                    peer,
                                    DocumentId::Catalog,
                                    Body::JoinRequest { transfer },
                                    preparation.vault_id,
                                );
                            }
                        }
                    }
                }
                Event::Disconnected { peer, session, .. } => {
                    if self
                        .peers
                        .get(&peer)
                        .is_some_and(|state| state.session == session)
                    {
                        self.revoke(&peer);
                    }
                }
                Event::Packet {
                    peer,
                    session,
                    packet,
                } => {
                    if self.denied.contains(&peer)
                        || !self
                            .peers
                            .get(&peer)
                            .is_some_and(|state| state.session == session)
                        || !self
                            .controller
                            .as_ref()
                            .is_some_and(|controller| controller.is_confirmed(&peer))
                    {
                        continue;
                    }
                    if let Err(error) = self.packet(&peer, session, packet, context, &mut actions) {
                        self.fail(&peer, error);
                    }
                    if actions
                        .iter()
                        .any(|action| matches!(action, Action::Receive { .. }))
                    {
                        break;
                    }
                }
                Event::SendAccepted {
                    peer,
                    session,
                    message_id,
                } => {
                    self.finish_send(&peer, session, message_id);
                }
                Event::SendRejected {
                    peer,
                    session,
                    message_id,
                    packet,
                    error,
                } => {
                    if !self.finish_send(&peer, session, message_id) {
                        continue;
                    }
                    if packet.validate(packet.vault_id).is_err() {
                        self.fail(&peer, error);
                    } else if self.peers.contains_key(&peer) && self.outbound.len() < 64 {
                        self.outbound.push_front((peer, packet));
                    } else {
                        self.fail(&peer, error);
                    }
                }
                Event::Failed(error) | Event::Rejected { error, .. } => self.error = Some(error),
            }
        }
        let peers: Vec<_> = self.peers.keys().cloned().collect();
        for peer in peers {
            if let Err(error) = self.drive(&peer, context) {
                self.fail(&peer, error);
            }
        }
        for _ in 0..16 {
            let Some((peer, packet)) = self.outbound.pop_front() else {
                break;
            };
            if self.sending.contains_key(&peer) {
                self.outbound.push_back((peer, packet));
                continue;
            }
            let Some(controller) = &self.controller else {
                self.outbound.push_front((peer, packet));
                break;
            };
            let Some(session) = self.peers.get(&peer).map(|state| state.session) else {
                continue;
            };
            let message_id = packet.message_id;
            match controller.try_send(Request::Send {
                session,
                peer: peer.clone(),
                packet,
            }) {
                Ok(()) => {
                    self.sending.insert(peer, (session, message_id));
                }
                Err(rejected) => {
                    if let Request::Send { packet, .. } = rejected.into_inner() {
                        self.outbound.push_front((peer, packet));
                    }
                    break;
                }
            }
        }
        actions
    }
    fn packet(
        &mut self,
        peer: &Fingerprint,
        session: usize,
        packet: Packet,
        context: &Context<'_>,
        actions: &mut Vec<Action>,
    ) -> Result<(), String> {
        let vault = context
            .history
            .map(|h| h.vault_id)
            .or_else(|| self.preparation.as_ref().map(|p| p.vault_id))
            .ok_or("no selected vault")?;
        packet.validate(vault).map_err(|e| e.to_string())?;
        match packet.body {
            Body::JoinRequest { transfer } => {
                let history = context.history.ok_or("source vault is not ready")?;
                let mut output = LimitedVec(Vec::new());
                serde_json::to_writer(&mut output, history)
                    .map_err(|_| "join history exceeds 32 MiB limit")?;
                let digest = Sha256::digest(&output.0).to_vec();
                self.outbound.retain(|(id, _)| id != peer);
                let state = self.peers.get_mut(peer).unwrap();
                state.out_join = Some(OutJoin {
                    transfer,
                    bytes: output.0,
                    digest,
                    next: 0,
                });
                state.join_wait = Some(transfer);
            }
            Body::JoinChunk {
                transfer,
                index,
                count,
                total,
                digest,
                bytes,
            } => {
                if context.history.is_some() || self.committing {
                    return Ok(());
                }
                let join = self.incoming.as_mut().ok_or("unexpected join transfer")?;
                if join.peer != *peer || join.session != session || join.transfer != transfer {
                    return Err("wrong join transfer/session".into());
                }
                if join.next == 0 {
                    join.count = count as usize;
                    join.total = total as usize;
                    join.digest = digest.clone();
                    if join.total == 0
                        || join.total > JOIN_LIMIT
                        || join.count != join.total.div_ceil(CHUNK)
                    {
                        return Err("invalid join length".into());
                    }
                }
                if index as usize != join.next
                    || count as usize != join.count
                    || total as usize != join.total
                    || digest != join.digest
                    || bytes.len() != CHUNK.min(join.total - join.bytes.len())
                {
                    return Err("inconsistent join chunk".into());
                }
                join.bytes.extend(bytes);
                join.next += 1;
                if join.next == join.count {
                    let join = self.incoming.take().unwrap();
                    if Sha256::digest(&join.bytes).as_slice() != join.digest {
                        return Err("join digest mismatch".into());
                    }
                    let mut bundle: HistoryBundle =
                        serde_json::from_slice(&join.bytes).map_err(|e| e.to_string())?;
                    let preparation = self.preparation.as_ref().ok_or("join not prepared")?;
                    if bundle.vault_id != preparation.vault_id {
                        return Err("wrong join vault".into());
                    }
                    bundle.device_id = preparation.device_id;
                    Replica::load(&bundle).map_err(|e| e.to_string())?;
                    self.committing = true;
                    actions.push(Action::Join {
                        peer: peer.clone(),
                        session,
                        transfer,
                        bundle,
                    });
                }
            }
            Body::DurableReceipt { received } => {
                if let Some(state) = self.peers.get_mut(peer) {
                    state.last_receipt = Some(received);
                    if state.join_wait == Some(received) {
                        state.join_wait = None;
                        state.out_join = None;
                        state.states.clear();
                        state.docs = None;
                        state.manifest_generation = None;
                    }
                }
            }
            Body::Manifest { .. } => {}
            Body::Sync { bytes } => {
                let Some(history) = context.history else {
                    return Ok(());
                };
                let state = self.peers.get_mut(peer).unwrap();
                if state.docs.is_none() {
                    state.docs = Some(Documents::load(history)?);
                    state.view_generation = context.persisted;
                }
                let changed = state.docs.as_mut().unwrap().receive(
                    packet.document,
                    &bytes,
                    state.states.entry(packet.document).or_default(),
                )?;
                if state.receipts.len() >= 64 {
                    return Err("receive receipt queue full".into());
                }
                state.receipts.push(packet.message_id);
                state.dirty |= changed;
                if changed {
                    state.changed.insert(packet.document);
                }
                if state.dirty {
                    let bundle = state.docs.as_mut().unwrap().bundle();
                    // Catalog and note messages may arrive in either order.
                    if Replica::load(&bundle).is_ok() {
                        let receipts = std::mem::take(&mut state.receipts);
                        state.dirty = false;
                        state.blocked = Some(u64::MAX);
                        actions.push(Action::Receive {
                            peer: peer.clone(),
                            bundle,
                            receipts,
                        });
                    }
                }
            }
        }
        Ok(())
    }
    fn drive(&mut self, peer: &Fingerprint, context: &Context<'_>) -> Result<(), String> {
        if self.sending.contains_key(peer)
            || self.denied.contains(peer)
            || self.outbound.iter().any(|(id, _)| id == peer)
        {
            return Ok(());
        }
        let state = self.peers.get_mut(peer).unwrap();
        if let Some(join) = &mut state.out_join {
            if join.next < join.bytes.len().div_ceil(CHUNK) {
                let index = join.next;
                let bytes =
                    join.bytes[index * CHUNK..((index + 1) * CHUNK).min(join.bytes.len())].to_vec();
                let body = Body::JoinChunk {
                    transfer: join.transfer,
                    index: index as u32,
                    count: join.bytes.len().div_ceil(CHUNK) as u32,
                    total: join.bytes.len() as u32,
                    digest: join.digest.clone(),
                    bytes,
                };
                join.next += 1;
                return self.enqueue(
                    peer.clone(),
                    DocumentId::Catalog,
                    body,
                    context.history.unwrap().vault_id,
                );
            }
        }
        if state.join_wait.is_some() {
            return Ok(());
        }
        if state.dirty {
            // Handshake-only responses for unchanged documents are safe. This
            // lets a newly catalogued document supply its missing history, while
            // withholding all acknowledgements of uncommitted changed docs.
            if let Some(docs) = &mut state.docs {
                for (id, doc) in &mut docs.docs {
                    if !state.changed.contains(id) {
                        let message = {
                            let protocol = doc.sync();
                            protocol.generate_sync_message(state.states.entry(*id).or_default())
                        };
                        if let Some(message) = message {
                            let id = *id;
                            return self.enqueue(
                                peer.clone(),
                                id,
                                Body::Sync {
                                    bytes: message.encode(),
                                },
                                context.history.unwrap().vault_id,
                            );
                        }
                    }
                }
            }
            return Ok(());
        }
        let Some(history) = context.history else {
            return Ok(());
        };
        if let Some(generation) = state.blocked {
            if context.persisted < generation {
                return Ok(());
            }
            state.blocked = None;
            state.docs = None;
            state.changed.clear();
        }
        if state.docs.is_none() || state.view_generation != context.persisted {
            state.docs = Some(Documents::load(history)?);
            state.view_generation = context.persisted;
        }
        if let Some(received) = state.receipts.pop() {
            return self.enqueue(
                peer.clone(),
                DocumentId::Catalog,
                Body::DurableReceipt { received },
                history.vault_id,
            );
        }
        let mut documents = vec![DocumentId::Catalog];
        documents.extend(history.documents.iter().map(|doc| DocumentId::Note(doc.id)));
        if state.manifest_generation != Some(context.persisted) {
            state.manifest_generation = Some(context.persisted);
            return self.enqueue(
                peer.clone(),
                DocumentId::Catalog,
                Body::Manifest {
                    notes: history.documents.iter().map(|doc| doc.id).collect(),
                },
                history.vault_id,
            );
        }
        for _ in 0..documents.len().min(8) {
            let id = documents[state.cursor % documents.len()];
            state.cursor += 1;
            let message = {
                let protocol = state
                    .docs
                    .as_mut()
                    .unwrap()
                    .docs
                    .get_mut(&id)
                    .unwrap()
                    .sync();
                protocol.generate_sync_message(state.states.entry(id).or_default())
            };
            if let Some(message) = message {
                return self.enqueue(
                    peer.clone(),
                    id,
                    Body::Sync {
                        bytes: message.encode(),
                    },
                    history.vault_id,
                );
            }
        }
        Ok(())
    }
}
struct LimitedVec(Vec<u8>);
impl std::io::Write for LimitedVec {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len() + bytes.len() > JOIN_LIMIT {
            return Err(std::io::Error::other("join history too large"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::{NoteId, NoteRecord, Revision, TextEdit};
    // Find the actual changes-bearing Automerge frame after the protocol handshake.
    fn frame(before: &[u8], after: &[u8]) -> (State, Vec<u8>) {
        let mut sender = load(after).unwrap();
        let mut receiver = load(before).unwrap();
        let mut sent = State::new();
        let mut received = State::new();
        for _ in 0..8 {
            if let Some(message) = sender.sync().generate_sync_message(&mut sent) {
                let old_state = received.clone();
                let heads = receiver.get_heads();
                let bytes = message.encode();
                receiver
                    .sync()
                    .receive_sync_message(&mut received, Message::decode(&bytes).unwrap())
                    .unwrap();
                if receiver.get_heads() != heads {
                    return (old_state, bytes);
                }
            }
            if let Some(message) = receiver.sync().generate_sync_message(&mut received) {
                sender
                    .sync()
                    .receive_sync_message(&mut sent, message)
                    .unwrap();
            }
        }
        panic!("missing changes frame")
    }
    #[test]
    fn receive_generation_barrier_keeps_later_incomplete_catalog_and_receipts_separate() {
        let id = NoteId(Uuid::new_v4());
        let note = NoteRecord {
            id,
            path: "a.md".into(),
            text: "before".into(),
            revision: Revision::fresh(),
            trashed: false,
        };
        let mut source = Replica::bootstrap(&[note]).unwrap();
        let base = source.save();
        source
            .edit(
                id,
                &[TextEdit {
                    start: 0,
                    end: 6,
                    replacement: "committed first".into(),
                }],
            )
            .unwrap();
        let first = source.save();
        let (state, bytes) = frame(&base.documents[0].bytes, &first.documents[0].bytes);
        let peer = Fingerprint("peer".into());
        let mut owner = LanState::default();
        let mut p = Peer::new(1);
        p.docs = Some(Documents::load(&base).unwrap());
        p.states.insert(DocumentId::Note(id), state);
        owner.peers.insert(peer.clone(), p);
        let context = Context {
            history: Some(&base),
            accepted: 1,
            persisted: 1,
        };
        let first_id = Uuid::new_v4();
        let mut actions = vec![];
        owner
            .packet(
                &peer,
                1,
                Packet {
                    version: 1,
                    vault_id: base.vault_id,
                    document: DocumentId::Note(id),
                    message_id: first_id,
                    body: Body::Sync { bytes },
                },
                &context,
                &mut actions,
            )
            .unwrap();
        assert!(matches!(&actions[..],[Action::Receive{receipts,..}] if receipts==&vec![first_id]));
        owner.received(&peer, 2, vec![first_id]);
        // A later catalog frame from the same event batch must remain unread;
        // subsequent ticks while generation 2 is pending do not reset its staging.
        for _ in 0..2 {
            assert!(owner.tick(&context).is_empty());
            assert_eq!(owner.peers[&peer].blocked, Some(2));
        }
        let mut later = Replica::load(&first).unwrap();
        later
            .insert(&NoteRecord {
                id: NoteId(Uuid::new_v4()),
                path: "new.md".into(),
                text: "not transferred yet".into(),
                revision: Revision::fresh(),
                trashed: false,
            })
            .unwrap();
        let complete = later.save();
        let (state, bytes) = frame(&first.catalog, &complete.catalog);
        let committed = Context {
            history: Some(&first),
            accepted: 2,
            persisted: 2,
        };
        owner.tick(&committed);
        owner
            .peers
            .get_mut(&peer)
            .unwrap()
            .states
            .insert(DocumentId::Catalog, state);
        let later_id = Uuid::new_v4();
        actions.clear();
        owner
            .packet(
                &peer,
                1,
                Packet {
                    version: 1,
                    vault_id: base.vault_id,
                    document: DocumentId::Catalog,
                    message_id: later_id,
                    body: Body::Sync { bytes },
                },
                &committed,
                &mut actions,
            )
            .unwrap();
        assert!(actions.is_empty());
        let p = &owner.peers[&peer];
        assert!(p.dirty);
        assert_eq!(p.receipts, vec![later_id]);
        assert!(
            Replica::load(
                &owner
                    .peers
                    .get_mut(&peer)
                    .unwrap()
                    .docs
                    .as_mut()
                    .unwrap()
                    .bundle()
            )
            .is_err()
        );
    }
    #[test]
    fn join_backpressure_allows_only_one_owned_chunk_in_flight() {
        let mut replica = Replica::bootstrap(&[]).unwrap();
        let history = replica.save();
        let peer = Fingerprint("peer".into());
        let mut owner = LanState::default();
        let mut state = Peer::new(1);
        state.out_join = Some(OutJoin {
            transfer: Uuid::new_v4(),
            bytes: vec![255; CHUNK * 3],
            digest: vec![0; 32],
            next: 0,
        });
        owner.peers.insert(peer.clone(), state);
        let context = Context {
            history: Some(&history),
            accepted: 0,
            persisted: 0,
        };
        owner.drive(&peer, &context).unwrap();
        let (_, first) = owner.outbound.pop_front().unwrap();
        assert!(matches!(first.body, Body::JoinChunk { index: 0, .. }));
        owner.sending.insert(peer.clone(), (1, first.message_id));
        // Until transport explicitly accepts/rejects this exact packet, no later
        // chunk can be generated, eliminating multiple-rejection reversal.
        owner.drive(&peer, &context).unwrap();
        assert!(owner.outbound.is_empty());
        assert_eq!(owner.peers[&peer].out_join.as_ref().unwrap().next, 1);
        owner.sending.remove(&peer);
        owner.outbound.push_front((peer.clone(), first));
        owner.drive(&peer, &context).unwrap();
        assert_eq!(owner.outbound.len(), 1);
        assert!(matches!(
            owner.outbound[0].1.body,
            Body::JoinChunk { index: 0, .. }
        ));
        let old_id = owner.outbound[0].1.message_id;
        owner.revoke(&peer);
        owner.peers.insert(peer.clone(), Peer::new(2));
        let current = Uuid::new_v4();
        owner.sending.insert(peer.clone(), (2, current));
        // Both a late acknowledgement and a late rejection share this exact
        // completion guard; neither can clear/requeue replacement-session work.
        assert!(!owner.finish_send(&peer, 1, old_id));
        assert!(!owner.finish_send(&peer, 2, old_id));
        assert_eq!(owner.sending[&peer], (2, current));
        assert!(owner.outbound.is_empty());
        assert!(owner.finish_send(&peer, 2, current));
    }
    #[test]
    fn retirement_cancels_controls_and_keeps_revocation_until_new_confirmation() {
        let peer = Fingerprint("peer".into());
        let mut owner = LanState::default();
        let old = Uuid::new_v4();
        owner.controls.insert(old, Some(peer.clone()));
        owner.deny(&peer);
        owner.pause();
        assert!(owner.denied.contains(&peer));
        assert!(!owner.controls.contains_key(&old));
        let actions = owner.tick(&Context {
            history: None,
            accepted: 0,
            persisted: 0,
        });
        assert!(matches!(&actions[..],[Action::Status(id,Err(_))] if *id==old));
    }
}
