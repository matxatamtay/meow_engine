//! Transport-neutral network broker boundary.

use std::{fmt, future::Future, pin::Pin, sync::Arc};

use meow_url_policy::BrowserUrl;

use crate::{CancellationToken, NetError, Request, Response};

/// Cookie/credential behavior attached to one brokered request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CredentialsMode {
    /// Do not send or store cookies.
    Omit,
    /// Send cookies only when the target is same-origin with the document.
    #[default]
    SameOrigin,
    /// Send and store cookies for permitted cross-origin requests.
    Include,
}

impl CredentialsMode {
    #[must_use]
    pub(crate) const fn allows(self, cross_origin: bool) -> bool {
        match self {
            Self::Omit => false,
            Self::SameOrigin => !cross_origin,
            Self::Include => true,
        }
    }
}

/// Security context supplied to the network owner.
#[derive(Clone, Debug, Default)]
pub struct RequestContext {
    pub document_url: Option<BrowserUrl>,
    pub credentials: CredentialsMode,
}

impl RequestContext {
    #[must_use]
    pub const fn anonymous() -> Self {
        Self {
            document_url: None,
            credentials: CredentialsMode::Omit,
        }
    }

    #[must_use]
    pub fn document(document_url: BrowserUrl, credentials: CredentialsMode) -> Self {
        Self {
            document_url: Some(document_url),
            credentials,
        }
    }
}

/// Backend implemented by a network process client or another permission mediator.
pub trait RequestBroker: fmt::Debug + Send + Sync + 'static {
    fn load<'a>(
        &'a self,
        request: Request,
        context: RequestContext,
        cancellation: &'a CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<Response, NetError>> + Send + 'a>>;
}

impl<T: RequestBroker> RequestBroker for Arc<T> {
    fn load<'a>(
        &'a self,
        request: Request,
        context: RequestContext,
        cancellation: &'a CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<Response, NetError>> + Send + 'a>> {
        (**self).load(request, context, cancellation)
    }
}
