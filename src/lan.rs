//! Authenticated QUIC I/O only. Automerge documents and sync State stay on App's owner.
use crate::notes::NoteId;
use quinn::{Connection, Endpoint};
use rustls::{
    RootCertStore,
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tokio::sync::{Semaphore, mpsc};
use uuid::Uuid;
const VERSION: u16 = 1;
const MAX_PAYLOAD: usize = 1024 * 1024;
// A JSON byte can occupy four characters ("255,"). Reserve fixed envelope
// overhead as well, so every permitted Sync payload has a permitted wire frame.
const MAX_FRAME: usize = 4 * MAX_PAYLOAD + 1024;
const MAX_PEERS: usize = 8;
const ALPN: &[u8] = b"tanglewood-sync/1";
const NAME: &str = "tanglewood.local";
#[derive(Debug, thiserror::Error)]
pub enum LanError {
    #[error("LAN error: {0}")]
    Io(String),
    #[error("invalid pairing ticket or protocol message")]
    Invalid,
    #[error("peer is not locally confirmed")]
    Unpaired,
    #[error("LAN queue is full")]
    Busy,
}
fn error(value: impl std::fmt::Display) -> LanError {
    LanError::Io(value.to_string())
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(pub String);
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    pub version: u16,
    pub address: SocketAddr,
    pub vault_id: Uuid,
    pub fingerprint: Fingerprint,
    pub certificate: Vec<u8>,
}
impl Ticket {
    pub fn parse(text: &str) -> Result<Self, LanError> {
        if text.len() > 16 * 1024 {
            return Err(LanError::Invalid);
        }
        let ticket: Self = serde_json::from_str(
            text.strip_prefix("tanglewood:1:")
                .ok_or(LanError::Invalid)?,
        )
        .map_err(|_| LanError::Invalid)?;
        ticket.validate()?;
        Ok(ticket)
    }
    pub fn encode(&self) -> Result<String, LanError> {
        self.validate()?;
        Ok(format!(
            "tanglewood:1:{}",
            serde_json::to_string(self).map_err(error)?
        ))
    }
    pub fn validate(&self) -> Result<(), LanError> {
        if self.version != VERSION
            || self.address.port() == 0
            || self.address.ip().is_unspecified()
            || self.address.ip().is_multicast()
            || self.vault_id.is_nil()
            || self.certificate.is_empty()
            || self.certificate.len() > 4096
            || fingerprint(&self.certificate)? != self.fingerprint
        {
            return Err(LanError::Invalid);
        }
        Ok(())
    }
}
fn fingerprint(certificate: &[u8]) -> Result<Fingerprint, LanError> {
    let der = CertificateDer::from(certificate);
    let parsed = rustls::server::ParsedCertificate::try_from(&der).map_err(error)?;
    let hash = Sha256::digest(parsed.subject_public_key_info().as_ref());
    Ok(Fingerprint(
        hash.iter().map(|byte| format!("{byte:02x}")).collect(),
    ))
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentId {
    Catalog,
    Note(NoteId),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Body {
    Manifest { notes: Vec<NoteId> },
    Sync { bytes: Vec<u8> },
    DurableReceipt { received: Uuid },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Packet {
    pub version: u16,
    pub vault_id: Uuid,
    pub document: DocumentId,
    pub message_id: Uuid,
    pub body: Body,
}
impl Packet {
    pub fn validate(&self, vault: Uuid) -> Result<(), LanError> {
        if self.version != VERSION
            || self.vault_id != vault
            || self.message_id.is_nil()
            || matches!(self.document,DocumentId::Note(id) if id.0.is_nil())
        {
            return Err(LanError::Invalid);
        }
        match &self.body {
            Body::Manifest { notes }
                if notes.len() > 10_000
                    || self.document != DocumentId::Catalog
                    || notes.iter().any(|id| id.0.is_nil())
                    || notes.iter().collect::<std::collections::HashSet<_>>().len()
                        != notes.len() =>
            {
                Err(LanError::Invalid)
            }
            Body::Sync { bytes } if bytes.len() > MAX_PAYLOAD => Err(LanError::Invalid),
            _ => Ok(()),
        }
    }
}
#[derive(Debug)]
pub enum Request {
    Confirm(Ticket),
    Unpair(Fingerprint),
    Connect(Ticket),
    Send { peer: Fingerprint, packet: Packet },
}
#[derive(Debug)]
pub enum Event {
    Confirmed(Fingerprint),
    Unpaired(Fingerprint),
    Connected {
        peer: Fingerprint,
        session: usize,
    },
    Packet {
        peer: Fingerprint,
        session: usize,
        packet: Packet,
    },
    Disconnected {
        peer: Fingerprint,
        session: usize,
        error: String,
    },
    Rejected {
        address: SocketAddr,
        error: String,
    },
    Failed(String),
    SendRejected {
        peer: Fingerprint,
        packet: Packet,
        error: String,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    certificate: Vec<u8>,
    private_key: Vec<u8>,
}
impl Identity {
    fn certificates(&self) -> Vec<CertificateDer<'static>> {
        vec![CertificateDer::from(self.certificate.clone())]
    }
    fn key(&self) -> rustls::pki_types::PrivateKeyDer<'static> {
        PrivatePkcs8KeyDer::from(self.private_key.clone()).into()
    }
}
#[derive(Clone)]
struct Session {
    connection: Connection,
    outbound: mpsc::Sender<Vec<u8>>,
}
type Sessions = Arc<Mutex<HashMap<Fingerprint, Session>>>;
type Pins = Arc<RwLock<HashMap<Fingerprint, Ticket>>>;
fn encode_packet(packet: &Packet, vault: Uuid) -> Result<Vec<u8>, LanError> {
    packet.validate(vault)?;
    let bytes = serde_json::to_vec(packet).map_err(error)?;
    if bytes.len() > MAX_FRAME {
        return Err(LanError::Invalid);
    }
    Ok(bytes)
}
fn register_session(
    pins: &Pins,
    sessions: &Sessions,
    peer: &Fingerprint,
    session: Session,
) -> Result<(), LanError> {
    // Keep the pin read lock through insertion. Unpair takes these same locks
    // in pins -> sessions order, so it either prevents registration or closes it.
    let confirmed = pins.read().map_err(error)?;
    if !confirmed.contains_key(peer) {
        return Err(LanError::Unpaired);
    }
    let mut active = sessions.lock().map_err(error)?;
    if active.contains_key(peer) {
        return Err(LanError::Io("duplicate peer session".into()));
    }
    active.insert(peer.clone(), session);
    Ok(())
}
fn revoke_session(
    pins: &Pins,
    sessions: &Sessions,
    updated: HashMap<Fingerprint, Ticket>,
    peer: &Fingerprint,
) -> Result<(), LanError> {
    let mut confirmed = pins.write().map_err(error)?;
    *confirmed = updated;
    let mut active = sessions.lock().map_err(error)?;
    if let Some(session) = active.remove(peer) {
        session.connection.close(0u32.into(), b"peer unpaired");
    }
    Ok(())
}
fn peer_sender(
    pins: &Pins,
    sessions: &Sessions,
    peer: &Fingerprint,
) -> Result<mpsc::Sender<Vec<u8>>, LanError> {
    let confirmed = pins.read().map_err(error)?;
    if !confirmed.contains_key(peer) {
        return Err(LanError::Unpaired);
    }
    sessions
        .lock()
        .map_err(error)?
        .get(peer)
        .map(|session| session.outbound.clone())
        .ok_or_else(|| LanError::Io("peer disconnected".into()))
}
#[cfg(test)]
struct RegistrationHook {
    peer: Fingerprint,
    entered: std::sync::mpsc::Sender<()>,
    resume: std::sync::mpsc::Receiver<()>,
}
#[cfg(test)]
static REGISTRATION_HOOK: Mutex<Option<RegistrationHook>> = Mutex::new(None);
#[cfg(test)]
fn pause_registration(peer: &Fingerprint) {
    let hook = {
        let mut hook = REGISTRATION_HOOK.lock().unwrap();
        if hook.as_ref().is_some_and(|hook| &hook.peer == peer) {
            hook.take()
        } else {
            None
        }
    };
    if let Some(hook) = hook {
        hook.entered.send(()).unwrap();
        hook.resume.recv_timeout(Duration::from_secs(5)).unwrap();
    }
}
/// Bounded transport handle. Call bind on the existing I/O Tokio runtime; owner
/// uses try_send/try_recv. No connection handler owns a Replica or sync State.
pub struct Controller {
    endpoint: Endpoint,
    requests: mpsc::Sender<Request>,
    events: mpsc::Receiver<Event>,
    task: tokio::task::JoinHandle<()>,
    ticket: Ticket,
}
impl Controller {
    pub async fn bind(
        vault_directory: &Path,
        vault_id: Uuid,
        address: SocketAddr,
    ) -> Result<Self, LanError> {
        let directory = vault_directory.join("lan");
        let (directory, identity, loaded) = tokio::task::spawn_blocking(move || {
            load_assets(&directory).map(|(identity, pins)| (directory, identity, pins))
        })
        .await
        .map_err(error)??;
        Self::start(directory, identity, loaded, vault_id, address).await
    }
    async fn start(
        directory: PathBuf,
        identity: Identity,
        loaded: Vec<Ticket>,
        vault_id: Uuid,
        address: SocketAddr,
    ) -> Result<Self, LanError> {
        // This function is async so Quinn always sees the caller's runtime.
        let endpoint = Endpoint::client(address).map_err(error)?;
        let local = endpoint.local_addr().map_err(error)?;
        let ticket = Ticket {
            version: VERSION,
            address: local,
            vault_id,
            fingerprint: fingerprint(&identity.certificate)?,
            certificate: identity.certificate.clone(),
        };
        let mut map = HashMap::new();
        for pin in loaded {
            pin.validate()?;
            if pin.vault_id != vault_id {
                return Err(LanError::Invalid);
            }
            map.insert(pin.fingerprint.clone(), pin);
        }
        if map.len() > MAX_PEERS {
            return Err(LanError::Busy);
        }
        let pins: Pins = Arc::new(RwLock::new(map));
        refresh_server(&endpoint, &identity, &pins)?;
        let (requests, receiver) = mpsc::channel(16);
        let (events, completions) = mpsc::channel(16);
        let task = tokio::spawn(run(
            endpoint.clone(),
            directory,
            Arc::new(identity),
            pins,
            vault_id,
            receiver,
            events,
        ));
        Ok(Self {
            endpoint,
            requests,
            events: completions,
            task,
            ticket,
        })
    }
    /// For a wildcard bind, caller must explicitly choose an advertised LAN IP.
    pub fn ticket(&self, advertised: Option<SocketAddr>) -> Result<Ticket, LanError> {
        let mut ticket = self.ticket.clone();
        if let Some(address) = advertised {
            if address.port() != ticket.address.port() {
                return Err(LanError::Invalid);
            }
            ticket.address = address;
        }
        ticket.validate()?;
        Ok(ticket)
    }
    pub fn try_send(&self, request: Request) -> Result<(), mpsc::error::TrySendError<Request>> {
        self.requests.try_send(request)
    }
    pub fn try_recv(&mut self) -> Option<Event> {
        self.events.try_recv().ok()
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.endpoint
            .close(0u32.into(), b"foreground session ended");
        self.task.abort();
    }
}
fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}
fn limits() -> Arc<quinn::TransportConfig> {
    let mut config = quinn::TransportConfig::default();
    config
        .max_concurrent_bidi_streams(1u32.into())
        .max_concurrent_uni_streams(0u32.into())
        .stream_receive_window((MAX_FRAME as u32).into())
        .receive_window((MAX_FRAME as u32 * 2).into())
        .max_idle_timeout(Some(
            Duration::from_secs(30)
                .try_into()
                .expect("valid idle timeout"),
        ))
        .keep_alive_interval(Some(Duration::from_secs(5)));
    Arc::new(config)
}
fn refresh_server(endpoint: &Endpoint, identity: &Identity, pins: &Pins) -> Result<(), LanError> {
    let pins = pins.read().map_err(error)?;
    if pins.is_empty() {
        endpoint.set_server_config(None);
        return Ok(());
    }
    let mut roots = RootCertStore::empty();
    for ticket in pins.values() {
        roots
            .add(CertificateDer::from(ticket.certificate.clone()))
            .map_err(error)?;
    }
    let verifier =
        rustls::server::WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider())
            .build()
            .map_err(error)?;
    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(error)?
        .with_client_cert_verifier(verifier)
        .with_single_cert(identity.certificates(), identity.key())
        .map_err(error)?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls).map_err(error)?;
    let mut server = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    server.transport_config(limits());
    endpoint.set_server_config(Some(server));
    Ok(())
}
fn client_config(identity: &Identity, ticket: &Ticket) -> Result<quinn::ClientConfig, LanError> {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(ticket.certificate.clone()))
        .map_err(error)?;
    let mut tls = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(error)?
        .with_root_certificates(roots)
        .with_client_auth_cert(identity.certificates(), identity.key())
        .map_err(error)?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls).map_err(error)?;
    let mut client = quinn::ClientConfig::new(Arc::new(crypto));
    client.transport_config(limits());
    Ok(client)
}
async fn run(
    endpoint: Endpoint,
    directory: PathBuf,
    identity: Arc<Identity>,
    pins: Pins,
    vault: Uuid,
    mut requests: mpsc::Receiver<Request>,
    events: mpsc::Sender<Event>,
) {
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
    let budget = Arc::new(Semaphore::new(MAX_PEERS));
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            Some(incoming)=endpoint.accept()=>{
                let address=incoming.remote_address();let Ok(permit)=budget.clone().try_acquire_owned()else{incoming.refuse();continue;};
                let(events,pins,sessions)=(events.clone(),pins.clone(),sessions.clone());
                tasks.spawn(async move{let _permit=permit;match incoming.await{Ok(connection)=>session(connection,false,None,vault,pins,sessions,events).await,Err(error)=>{let _=emit(&events,Event::Rejected{address,error:error.to_string()}).await;}}});
            }
            request=requests.recv()=>{let Some(request)=request else{break;};match request{
                Request::Confirm(ticket)=>{
                    if ticket.validate().is_err()||ticket.vault_id!=vault{let _=emit(&events,Event::Failed("invalid or different-vault ticket".into())).await;continue;}
                    let mut updated=match pins.read(){Ok(pins)=>pins.clone(),Err(_)=>break,};
                    if updated.len()>=MAX_PEERS&&!updated.contains_key(&ticket.fingerprint){let _=emit(&events,Event::Failed("peer limit reached".into())).await;continue;}
                    updated.insert(ticket.fingerprint.clone(),ticket.clone());let path=directory.join("peers.json");let snapshot:Vec<_>=updated.values().cloned().collect();
                    let persisted=tokio::task::spawn_blocking(move||write_private(&path,&serde_json::to_vec(&snapshot).map_err(error)?)).await;
                    match persisted{Ok(Ok(()))=>{if let Ok(mut pins)=pins.write(){*pins=updated;}if let Err(error)=refresh_server(&endpoint,&identity,&pins){let _=emit(&events,Event::Failed(error.to_string())).await;}else{let _=emit(&events,Event::Confirmed(ticket.fingerprint)).await;}},other=>{let _=emit(&events,Event::Failed(format!("pin persistence failed: {}",match other{Ok(Err(error))=>error.to_string(),Err(error)=>error.to_string(),_=>unreachable!()}))).await;}}
                }
                Request::Unpair(peer)=>{
                    let mut updated=match pins.read(){Ok(pins)=>pins.clone(),Err(_)=>break,};updated.remove(&peer);let path=directory.join("peers.json");let snapshot:Vec<_>=updated.values().cloned().collect();
                    match tokio::task::spawn_blocking(move||write_private(&path,&serde_json::to_vec(&snapshot).map_err(error)?)).await{
                        Ok(Ok(()))=>{match revoke_session(&pins,&sessions,updated,&peer){Ok(())=>{let _=refresh_server(&endpoint,&identity,&pins);let _=emit(&events,Event::Unpaired(peer)).await;},Err(error)=>{let _=emit(&events,Event::Failed(error.to_string())).await;}}}
                        _=>{let _=emit(&events,Event::Failed("unpair persistence failed".into())).await;}
                    }
                }
                Request::Connect(ticket)=>{
                    let allowed=ticket.validate().is_ok()&&ticket.vault_id==vault&&pins.read().is_ok_and(|pins|pins.contains_key(&ticket.fingerprint));
                    if !allowed{let _=emit(&events,Event::Failed("peer is not locally confirmed".into())).await;continue;}
                    let Ok(permit)=budget.clone().try_acquire_owned()else{let _=emit(&events,Event::Failed("connection limit reached".into())).await;continue;};
                    let connecting=client_config(&identity,&ticket).and_then(|config|endpoint.connect_with(config,ticket.address,NAME).map_err(error));
                    let(events,pins,sessions)=(events.clone(),pins.clone(),sessions.clone());
                    tasks.spawn(async move{let _permit=permit;match connecting{Ok(connecting)=>match tokio::time::timeout(Duration::from_secs(10),connecting).await{Ok(Ok(connection))=>session(connection,true,Some(ticket.fingerprint.clone()),vault,pins,sessions,events).await,other=>{let _=emit(&events,Event::Rejected{address:ticket.address,error:format!("TLS connect failed: {other:?}")}).await;}},Err(error)=>{let _=emit(&events,Event::Failed(error.to_string())).await;}}});
                }
                Request::Send{peer,packet}=>{
                    let bytes=match encode_packet(&packet,vault){Ok(bytes)=>bytes,Err(error)=>{let _=emit(&events,Event::SendRejected{peer,packet,error:error.to_string()}).await;continue;}};
                    match peer_sender(&pins,&sessions,&peer){
                        Ok(sender)=>if sender.try_send(bytes).is_err(){let _=emit(&events,Event::SendRejected{peer,packet,error:"peer queue full or disconnected".into()}).await;},
                        Err(error)=>{let _=emit(&events,Event::SendRejected{peer,packet,error:error.to_string()}).await;}
                    }
                }
            }}
            Some(_)=tasks.join_next(),if !tasks.is_empty()=>{}
        }
    }
    endpoint.close(0u32.into(), b"transport closed");
    tasks.abort_all();
}
async fn emit(events: &mpsc::Sender<Event>, event: Event) -> Result<(), LanError> {
    tokio::time::timeout(Duration::from_secs(3), events.send(event))
        .await
        .map_err(error)?
        .map_err(|_| LanError::Io("owner disconnected".into()))
}
async fn session(
    connection: Connection,
    outgoing: bool,
    expected: Option<Fingerprint>,
    vault: Uuid,
    pins: Pins,
    sessions: Sessions,
    events: mpsc::Sender<Event>,
) {
    let authenticated = (|| {
        let certs = connection
            .peer_identity()
            .and_then(|identity| identity.downcast::<Vec<CertificateDer<'static>>>().ok())
            .ok_or(LanError::Unpaired)?;
        let peer = fingerprint(certs.first().ok_or(LanError::Unpaired)?.as_ref())?;
        if expected.as_ref().is_some_and(|expected| expected != &peer)
            || !pins.read().map_err(error)?.contains_key(&peer)
        {
            return Err(LanError::Unpaired);
        }
        Ok(peer)
    })();
    let peer = match authenticated {
        Ok(peer) => peer,
        Err(error) => {
            connection.close(1u32.into(), b"unpinned identity");
            let _ = emit(
                &events,
                Event::Rejected {
                    address: connection.remote_address(),
                    error: error.to_string(),
                },
            )
            .await;
            return;
        }
    };
    #[cfg(test)]
    pause_registration(&peer);
    let (outbound, mut outgoing_packets) = mpsc::channel(8);
    if let Err(error) = register_session(
        &pins,
        &sessions,
        &peer,
        Session {
            connection: connection.clone(),
            outbound,
        },
    ) {
        connection.close(1u32.into(), b"peer registration rejected");
        let _ = emit(
            &events,
            Event::Rejected {
                address: connection.remote_address(),
                error: error.to_string(),
            },
        )
        .await;
        return;
    }
    if emit(
        &events,
        Event::Connected {
            peer: peer.clone(),
            session: connection.stable_id(),
        },
    )
    .await
    .is_err()
    {
        connection.close(1u32.into(), b"owner unavailable");
        if let Ok(mut sessions) = sessions.lock() {
            sessions.remove(&peer);
        }
        return;
    }
    let result = async {
        let (mut send, mut recv) = if outgoing {
            connection.open_bi().await.map_err(error)?
        } else {
            tokio::time::timeout(Duration::from_secs(10), connection.accept_bi())
                .await
                .map_err(error)?
                .map_err(error)?
        };
        let reader = async {
            loop {
                let mut header = [0; 4];
                recv.read_exact(&mut header).await.map_err(error)?;
                let length = u32::from_be_bytes(header) as usize;
                if length == 0 || length > MAX_FRAME {
                    return Err::<(), LanError>(LanError::Invalid);
                }
                let mut bytes = vec![0; length];
                tokio::time::timeout(Duration::from_secs(10), recv.read_exact(&mut bytes))
                    .await
                    .map_err(error)?
                    .map_err(error)?;
                let packet: Packet = serde_json::from_slice(&bytes).map_err(error)?;
                packet.validate(vault)?;
                if !pins.read().map_err(error)?.contains_key(&peer) {
                    return Err(LanError::Unpaired);
                }
                emit(
                    &events,
                    Event::Packet {
                        peer: peer.clone(),
                        session: connection.stable_id(),
                        packet,
                    },
                )
                .await?;
            }
        };
        let writer = async {
            while let Some(bytes) = outgoing_packets.recv().await {
                tokio::time::timeout(Duration::from_secs(10), async {
                    send.write_all(&(bytes.len() as u32).to_be_bytes())
                        .await
                        .map_err(error)?;
                    send.write_all(&bytes).await.map_err(error)
                })
                .await
                .map_err(error)??;
            }
            Ok(())
        };
        tokio::select! {result=reader=>result,result=writer=>result}
    }
    .await;
    connection.close(0u32.into(), b"session ended");
    if let Ok(mut sessions) = sessions.lock() {
        if sessions
            .get(&peer)
            .is_some_and(|session| session.connection.stable_id() == connection.stable_id())
        {
            sessions.remove(&peer);
        }
    }
    let _ = emit(
        &events,
        Event::Disconnected {
            peer,
            session: connection.stable_id(),
            error: result
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default(),
        },
    )
    .await;
}
fn read_private(path: &Path) -> Result<Vec<u8>, LanError> {
    let metadata = fs::symlink_metadata(path).map_err(error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 128 * 1024 {
        return Err(LanError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(LanError::Io(
                "device identity/pins must have mode 600".into(),
            ));
        }
    }
    fs::read(path).map_err(error)
}
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), LanError> {
    let parent = path.parent().ok_or(LanError::Invalid)?;
    let temporary = parent.join(format!(".lan-{}.tmp", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary).map_err(error)?;
        file.write_all(bytes).map_err(error)?;
        file.sync_all().map_err(error)?;
        fs::rename(&temporary, path).map_err(error)?;
        fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn load_assets(directory: &Path) -> Result<(Identity, Vec<Ticket>), LanError> {
    if fs::symlink_metadata(directory).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(LanError::Invalid);
    }
    fs::create_dir_all(directory).map_err(error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(error)?;
    }
    let path = directory.join("identity.json");
    let identity = if path.exists() {
        serde_json::from_slice(&read_private(&path)?).map_err(error)?
    } else {
        let generated = rcgen::generate_simple_self_signed(vec![NAME.into()]).map_err(error)?;
        let identity = Identity {
            certificate: generated.cert.der().to_vec(),
            private_key: generated.signing_key.serialize_der(),
        };
        let bytes = serde_json::to_vec(&identity).map_err(error)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(error)?;
        file.write_all(&bytes).map_err(error)?;
        file.sync_all().map_err(error)?;
        fs::File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(error)?;
        identity
    };
    fingerprint(&identity.certificate)?;
    let pins = directory.join("peers.json");
    let pins = if fs::symlink_metadata(&pins).is_ok() {
        serde_json::from_slice(&read_private(&pins)?).map_err(error)?
    } else {
        Vec::new()
    };
    Ok((identity, pins))
}

#[cfg(test)]
mod tests {
    use super::*;
    use automerge::{
        AutoCommit, ObjType, ROOT, ReadDoc,
        sync::{Message, State, SyncDoc},
        transaction::Transactable,
    };
    async fn next(controller: &mut Controller) -> Event {
        tokio::time::timeout(Duration::from_secs(5), controller.events.recv())
            .await
            .expect("event timeout")
            .expect("controller disconnected")
    }
    async fn packet(controller: &mut Controller) -> (Fingerprint, Packet) {
        loop {
            match next(controller).await {
                Event::Packet { peer, packet, .. } => return (peer, packet),
                Event::Failed(error) | Event::Rejected { error, .. } => panic!("{error}"),
                _ => {}
            }
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn loopback_mutual_pins_real_sync_frames_unknown_peer_and_revocation() {
        let root = std::env::temp_dir().join(format!("tanglewood-lan-{}", Uuid::new_v4()));
        let vault = Uuid::new_v4();
        let bind = "127.0.0.1:0".parse().unwrap();
        let mut a = Controller::bind(&root.join("a"), vault, bind)
            .await
            .unwrap();
        let mut b = Controller::bind(&root.join("b"), vault, bind)
            .await
            .unwrap();
        let mut unknown = Controller::bind(&root.join("unknown"), vault, bind)
            .await
            .unwrap();
        let at = a.ticket(None).unwrap();
        let bt = b.ticket(None).unwrap();
        assert_eq!(
            Ticket::parse(&at.encode().unwrap()).unwrap().fingerprint,
            at.fingerprint
        );
        let mut tampered = at.clone();
        tampered.fingerprint = bt.fingerprint.clone();
        assert!(tampered.validate().is_err());
        a.try_send(Request::Confirm(bt.clone())).unwrap();
        b.try_send(Request::Confirm(at.clone())).unwrap();
        assert!(matches!(next(&mut a).await, Event::Confirmed(_)));
        assert!(matches!(next(&mut b).await, Event::Confirmed(_)));
        a.try_send(Request::Connect(bt.clone())).unwrap();
        assert!(matches!(next(&mut a).await, Event::Connected { .. }));
        assert!(matches!(next(&mut b).await, Event::Connected { .. }));
        let mut left = AutoCommit::new();
        let text = left.put_object(ROOT, "text", ObjType::Text).unwrap();
        left.splice_text(&text, 0, 0, "authenticated 🦀").unwrap();
        let mut right = AutoCommit::new();
        let (mut ls, mut rs) = (State::new(), State::new());
        for _ in 0..6 {
            if let Some(message) = left.sync().generate_sync_message(&mut ls) {
                a.try_send(Request::Send {
                    peer: bt.fingerprint.clone(),
                    packet: Packet {
                        version: VERSION,
                        vault_id: vault,
                        document: DocumentId::Catalog,
                        message_id: Uuid::new_v4(),
                        body: Body::Sync {
                            bytes: message.encode(),
                        },
                    },
                })
                .unwrap();
                let (peer, frame) = packet(&mut b).await;
                assert_eq!(peer, at.fingerprint);
                let Body::Sync { bytes } = frame.body else {
                    panic!("sync frame")
                };
                right
                    .sync()
                    .receive_sync_message(&mut rs, Message::decode(&bytes).unwrap())
                    .unwrap();
            }
            if let Some(message) = right.sync().generate_sync_message(&mut rs) {
                b.try_send(Request::Send {
                    peer: at.fingerprint.clone(),
                    packet: Packet {
                        version: VERSION,
                        vault_id: vault,
                        document: DocumentId::Catalog,
                        message_id: Uuid::new_v4(),
                        body: Body::Sync {
                            bytes: message.encode(),
                        },
                    },
                })
                .unwrap();
                let (_, frame) = packet(&mut a).await;
                let Body::Sync { bytes } = frame.body else {
                    panic!("sync frame")
                };
                left.sync()
                    .receive_sync_message(&mut ls, Message::decode(&bytes).unwrap())
                    .unwrap();
            }
        }
        let (_, right_text) = right.get(ROOT, "text").unwrap().unwrap();
        assert_eq!(right.text(right_text).unwrap(), "authenticated 🦀");
        // Every allowed byte value, at the maximum payload length, must fit
        // its encoded frame and arrive without consuming/dropping the packet.
        let maximum = Packet {
            version: VERSION,
            vault_id: vault,
            document: DocumentId::Catalog,
            message_id: Uuid::new_v4(),
            body: Body::Sync {
                bytes: vec![255; MAX_PAYLOAD],
            },
        };
        assert!(maximum.validate(vault).is_ok());
        assert!(encode_packet(&maximum, vault).unwrap().len() <= MAX_FRAME);
        let maximum_id = maximum.message_id;
        a.try_send(Request::Send {
            peer: bt.fingerprint.clone(),
            packet: maximum,
        })
        .unwrap();
        let (_, received) = packet(&mut b).await;
        assert_eq!(received.message_id, maximum_id);
        assert!(
            matches!(received.body,Body::Sync{bytes} if bytes.len()==MAX_PAYLOAD&&bytes.iter().all(|byte|*byte==255))
        );
        let oversize = Packet {
            version: VERSION,
            vault_id: vault,
            document: DocumentId::Catalog,
            message_id: Uuid::new_v4(),
            body: Body::Sync {
                bytes: vec![0; MAX_PAYLOAD + 1],
            },
        };
        assert!(oversize.validate(vault).is_err());
        a.try_send(Request::Send {
            peer: bt.fingerprint.clone(),
            packet: oversize,
        })
        .unwrap();
        match next(&mut a).await {
            Event::SendRejected { packet, .. } => {
                assert!(matches!(packet.body,Body::Sync{bytes} if bytes.len()==MAX_PAYLOAD+1))
            }
            other => panic!("expected retained rejected packet: {other:?}"),
        }
        unknown.try_send(Request::Confirm(at.clone())).unwrap();
        assert!(matches!(next(&mut unknown).await, Event::Confirmed(_)));
        unknown.try_send(Request::Connect(at.clone())).unwrap();
        assert!(matches!(next(&mut a).await, Event::Rejected { .. }));
        a.try_send(Request::Unpair(bt.fingerprint.clone())).unwrap();
        loop {
            if matches!(next(&mut a).await, Event::Unpaired(_)) {
                break;
            }
        }
        loop {
            if matches!(next(&mut b).await, Event::Disconnected { .. }) {
                break;
            }
        }
        // A confirmed, fully authenticated peer sending an oversized length is
        // rejected before allocating its advertised body or delivering a packet.
        a.try_send(Request::Confirm(bt.clone())).unwrap();
        loop {
            if matches!(next(&mut a).await, Event::Confirmed(_)) {
                break;
            }
        }
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        *REGISTRATION_HOOK.lock().unwrap() = Some(RegistrationHook {
            peer: bt.fingerprint.clone(),
            entered: entered_tx,
            resume: resume_rx,
        });
        let (identity, _) = load_assets(&root.join("b/lan")).unwrap();
        let raw = Endpoint::client(bind).unwrap();
        let raw_connection = raw
            .connect_with(client_config(&identity, &at).unwrap(), at.address, NAME)
            .unwrap()
            .await
            .unwrap();
        tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(5)))
            .await
            .unwrap()
            .unwrap();
        // Reproduce the old check/insert window: revoke after the handshake's
        // first pin check, before it is permitted to register its live session.
        a.try_send(Request::Unpair(bt.fingerprint.clone())).unwrap();
        loop {
            if matches!(next(&mut a).await, Event::Unpaired(_)) {
                break;
            }
        }
        let denied = Packet {
            version: VERSION,
            vault_id: vault,
            document: DocumentId::Catalog,
            message_id: Uuid::new_v4(),
            body: Body::Sync { bytes: vec![1, 2] },
        };
        let denied_id = denied.message_id;
        a.try_send(Request::Send {
            peer: bt.fingerprint.clone(),
            packet: denied,
        })
        .unwrap();
        loop {
            if let Event::SendRejected { packet, error, .. } = next(&mut a).await {
                assert_eq!(packet.message_id, denied_id);
                assert!(error.contains("not locally confirmed"));
                break;
            }
        }
        resume_tx.send(()).unwrap();
        loop {
            match next(&mut a).await {
                Event::Rejected { .. } => break,
                Event::Connected { .. } | Event::Packet { .. } => {
                    panic!("revoked handshake registered after Unpaired")
                }
                _ => {}
            }
        }
        tokio::time::timeout(Duration::from_secs(5), raw_connection.closed())
            .await
            .unwrap();
        a.try_send(Request::Confirm(bt.clone())).unwrap();
        loop {
            if matches!(next(&mut a).await, Event::Confirmed(_)) {
                break;
            }
        }
        let raw_connection = raw
            .connect_with(client_config(&identity, &at).unwrap(), at.address, NAME)
            .unwrap()
            .await
            .unwrap();
        loop {
            if matches!(next(&mut a).await, Event::Connected { .. }) {
                break;
            }
        }
        let (mut send, _recv) = raw_connection.open_bi().await.unwrap();
        send.write_all(&((MAX_FRAME + 1) as u32).to_be_bytes())
            .await
            .unwrap();
        loop {
            match next(&mut a).await {
                Event::Disconnected { error, .. } => {
                    assert!(error.contains("invalid pairing ticket"));
                    break;
                }
                Event::Packet { .. } => panic!("oversized frame delivered"),
                _ => {}
            }
        }
        raw.close(0u32.into(), b"test complete");
        a.try_send(Request::Unpair(bt.fingerprint.clone())).unwrap();
        loop {
            if matches!(next(&mut a).await, Event::Unpaired(_)) {
                break;
            }
        }
        assert!(read_private(&root.join("a/lan/identity.json")).is_ok());
        let saved_identity = at.fingerprint.clone();
        drop(a);
        drop(b);
        drop(unknown);
        let a = Controller::bind(&root.join("a"), vault, bind)
            .await
            .unwrap();
        assert_eq!(a.ticket(None).unwrap().fingerprint, saved_identity);
        let stored: Vec<Ticket> =
            serde_json::from_slice(&read_private(&root.join("a/lan/peers.json")).unwrap()).unwrap();
        assert!(stored.is_empty());
        drop(a);
        fs::remove_dir_all(root).unwrap();
    }
}
