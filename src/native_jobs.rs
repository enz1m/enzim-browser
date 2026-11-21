use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::oneshot;

#[derive(Clone)]
pub(crate) enum Payload {
    SecretJson(enzui_security::secret::SecretText),
    Json(serde_json::Value),
    Cookies(Vec<enzui_webkit::ffi::runtime::CookieInfo>),
    Pdf(enzui_webkit::printing::PdfDocument),
    Bytes(Arc<Vec<u8>>),
}
impl std::fmt::Debug for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SecretJson(_) => f.write_str("SecretJson([redacted])"),
            Self::Json(_) => f.write_str("Json(..)"),
            Self::Cookies(rows) => f.debug_tuple("Cookies").field(&rows.len()).finish(),
            Self::Pdf(document) => document.fmt(f),
            Self::Bytes(bytes) => f.debug_tuple("Bytes").field(&bytes.len()).finish(),
        }
    }
}
type Reply = Result<Payload, String>;
static NEXT_JOB: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(crate) struct Job {
    pub(crate) id: u64,
    sender: Arc<Mutex<Option<oneshot::Sender<Reply>>>>,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeJob")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Job {
    pub(crate) fn new() -> (Self, oneshot::Receiver<Reply>) {
        let (sender, receiver) = oneshot::channel();
        (
            Self {
                id: NEXT_JOB.fetch_add(1, Ordering::Relaxed),
                sender: Arc::new(Mutex::new(Some(sender))),
            },
            receiver,
        )
    }
    pub(crate) fn complete(&self, result: Reply) {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(sender) = sender {
            let _ = sender.send(result);
        }
    }
    pub(crate) fn closed(&self) -> bool {
        self.sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_none_or(oneshot::Sender::is_closed)
    }
}

pub(crate) async fn receive(receiver: oneshot::Receiver<Reply>) -> Reply {
    match tokio::time::timeout(std::time::Duration::from_secs(20), receiver).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("The page closed before the operation finished.".into()),
        Err(_) => Err("The page did not respond in time.".into()),
    }
}
