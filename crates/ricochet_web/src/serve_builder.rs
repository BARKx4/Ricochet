use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use axum::Router;

use crate::database_capability::DatabaseBackend;
use crate::server::{self, RequestFaultSink, ServeOptions, WatchTraceSink};

pub struct ServeBuilder {
    project_root: PathBuf,
    options: Option<ServeOptions>,
    database_backend: Option<Arc<dyn DatabaseBackend>>,
    trace_sink: Option<WatchTraceSink>,
    request_fault_sink: Option<RequestFaultSink>,
    watched: bool,
}

pub(crate) struct ServeBuilderParts {
    pub(crate) project_root: PathBuf,
    pub(crate) options: Option<ServeOptions>,
    pub(crate) database_backend: Option<Arc<dyn DatabaseBackend>>,
    pub(crate) trace_sink: Option<WatchTraceSink>,
    pub(crate) request_fault_sink: Option<RequestFaultSink>,
    pub(crate) watched: bool,
}

impl ServeBuilder {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            project_root: project_root.as_ref().to_path_buf(),
            options: None,
            database_backend: None,
            trace_sink: None,
            request_fault_sink: None,
            watched: false,
        }
    }

    pub fn options(mut self, options: ServeOptions) -> Self {
        self.options = Some(options);
        self
    }

    pub fn database_backend(mut self, backend: Arc<dyn DatabaseBackend>) -> Self {
        self.database_backend = Some(backend);
        self
    }

    pub fn watched(mut self, watched: bool) -> Self {
        self.watched = watched;
        self
    }

    pub fn trace_sink(mut self, trace_sink: WatchTraceSink) -> Self {
        self.trace_sink = Some(trace_sink);
        self
    }

    pub fn request_fault_sink(mut self, sink: RequestFaultSink) -> Self {
        self.request_fault_sink = Some(sink);
        self
    }

    pub fn build(self) -> Result<Router> {
        server::build_app_from_serve_builder(self)
    }

    pub(crate) fn request_fault_sink_option(mut self, sink: Option<RequestFaultSink>) -> Self {
        self.request_fault_sink = sink;
        self
    }

    pub(crate) fn into_parts(self) -> ServeBuilderParts {
        ServeBuilderParts {
            project_root: self.project_root,
            options: self.options,
            database_backend: self.database_backend,
            trace_sink: self.trace_sink,
            request_fault_sink: self.request_fault_sink,
            watched: self.watched,
        }
    }
}
