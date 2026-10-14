//! Shared SSE response adapter for the backend and stateless Agent.
use crate::{ai::streaming::AnalysisEvent, AppResult};
use std::{future::Future, pin::Pin};

// Own the execution future in the response body: dropping the connection cancels
// provider IO and Codex subprocesses, without leaving detached analysis jobs.
pub(crate) fn response_stream(
    future: Pin<Box<dyn Future<Output = AppResult<crate::models::AnalysisResult>> + Send>>,
    mut receiver: tokio::sync::mpsc::Receiver<AnalysisEvent>,
) -> impl futures_core::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>> + Send
{
    async_stream::stream! {
        let mut future = future;
        loop {
            let event = tokio::select! {
                biased;
                Some(event) = receiver.recv() => event,
                result = &mut future => {
                    let event = match result {
                        Ok(result) => AnalysisEvent::Complete { result: Box::new(result) },
                        Err(error) => AnalysisEvent::Error { message: error.to_string() },
                    };
                    yield Ok(axum::response::sse::Event::default().json_data(event).unwrap());
                    break;
                }
            };
            yield Ok(axum::response::sse::Event::default().json_data(event).unwrap());
        }
    }
}
