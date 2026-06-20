use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use tempfile::TempPath;

const DEFAULT_MAX_RETAINED_UPLOAD_STREAMS: usize = 64;

#[derive(Clone)]
pub struct UploadStreamRegistry {
    inner: Arc<Mutex<UploadStreamRegistryState>>,
}

struct UploadStreamRegistryState {
    next_id: u64,
    max_retained: usize,
    streams: BTreeMap<u64, Arc<UploadStream>>,
}

struct UploadStream {
    metadata: UploadStreamMetadata,
    path: TempPath,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadStreamMetadata {
    pub field: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadStreamSnapshot {
    pub id: u64,
    pub field: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub size_known: bool,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadStreamRead {
    pub snapshot: UploadStreamSnapshot,
    pub offset: u64,
    pub next_offset: u64,
    pub eof: bool,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadStreamRuntimeError {
    pub kind: &'static str,
    pub message: String,
}

impl UploadStreamRuntimeError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl UploadStreamRegistry {
    pub fn new() -> Self {
        Self::with_max_retained(DEFAULT_MAX_RETAINED_UPLOAD_STREAMS)
    }

    pub fn with_max_retained(max_retained: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(UploadStreamRegistryState {
                next_id: 0,
                max_retained,
                streams: BTreeMap::new(),
            })),
        }
    }

    pub fn retain_temp_file(
        &self,
        metadata: UploadStreamMetadata,
        path: TempPath,
    ) -> Result<UploadStreamSnapshot, UploadStreamRuntimeError> {
        let mut state = self
            .inner
            .lock()
            .expect("upload stream registry lock should not be poisoned");
        if state.streams.len() >= state.max_retained {
            return Err(UploadStreamRuntimeError::new(
                "RegistryFull",
                format!(
                    "upload stream registry retained stream limit of {} reached; release upload streams before accepting another upload",
                    state.max_retained
                ),
            ));
        }
        let id = state.next_id;
        state.next_id += 1;
        let stream = Arc::new(UploadStream { metadata, path });
        let snapshot = stream.snapshot(id);
        state.streams.insert(id, stream);
        Ok(snapshot)
    }

    pub fn streams(&self) -> Vec<UploadStreamSnapshot> {
        self.inner
            .lock()
            .expect("upload stream registry lock should not be poisoned")
            .streams
            .iter()
            .map(|(id, stream)| stream.snapshot(*id))
            .collect()
    }

    pub fn stream(&self, id: u64) -> Option<UploadStreamSnapshot> {
        self.stream_arc(id).map(|stream| stream.snapshot(id))
    }

    pub fn read(
        &self,
        id: u64,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Option<UploadStreamRead>, UploadStreamRuntimeError> {
        let Some(stream) = self.stream_arc(id) else {
            return Ok(None);
        };
        stream.read(id, offset, max_bytes).map(Some)
    }

    pub fn release(&self, id: u64) -> bool {
        self.inner
            .lock()
            .expect("upload stream registry lock should not be poisoned")
            .streams
            .remove(&id)
            .is_some()
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("upload stream registry lock should not be poisoned")
            .streams
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn stream_arc(&self, id: u64) -> Option<Arc<UploadStream>> {
        self.inner
            .lock()
            .expect("upload stream registry lock should not be poisoned")
            .streams
            .get(&id)
            .cloned()
    }
}

impl Default for UploadStreamRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl UploadStream {
    fn snapshot(&self, id: u64) -> UploadStreamSnapshot {
        UploadStreamSnapshot {
            id,
            field: self.metadata.field.clone(),
            filename: self.metadata.filename.clone(),
            content_type: self.metadata.content_type.clone(),
            size_known: true,
            size: self.metadata.size,
        }
    }

    fn read(
        &self,
        id: u64,
        offset: u64,
        max_bytes: usize,
    ) -> Result<UploadStreamRead, UploadStreamRuntimeError> {
        let mut file = fs::File::open(&self.path)
            .map_err(|error| UploadStreamRuntimeError::new("IoError", error.to_string()))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| UploadStreamRuntimeError::new("IoError", error.to_string()))?;
        let mut bytes = vec![0; max_bytes];
        let read = file
            .read(&mut bytes)
            .map_err(|error| UploadStreamRuntimeError::new("IoError", error.to_string()))?;
        bytes.truncate(read);
        let next_offset = offset.saturating_add(read as u64);
        let eof = next_offset >= self.metadata.size;
        Ok(UploadStreamRead {
            snapshot: self.snapshot(id),
            offset,
            next_offset,
            eof,
            bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn retained_upload_stream_reads_and_releases_temp_file() {
        let registry = UploadStreamRegistry::with_max_retained(1);
        let mut temp = tempfile::NamedTempFile::new().expect("temp upload file");
        temp.write_all(b"hello uploaded stream")
            .expect("write upload bytes");
        let temp_path = temp.into_temp_path();
        let disk_path = temp_path.to_path_buf();

        let snapshot = registry
            .retain_temp_file(
                UploadStreamMetadata {
                    field: "file".to_string(),
                    filename: Some("note.txt".to_string()),
                    content_type: Some("text/plain".to_string()),
                    size: 21,
                },
                temp_path,
            )
            .expect("stream should retain");

        assert_eq!(snapshot.id, 0);
        assert_eq!(registry.len(), 1);

        let read = registry
            .read(snapshot.id, 6, 8)
            .expect("read should not fail")
            .expect("stream should exist");
        assert_eq!(read.bytes, b"uploaded");
        assert_eq!(read.next_offset, 14);
        assert!(!read.eof);

        assert!(registry.release(snapshot.id));
        assert!(!disk_path.exists());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn retained_upload_stream_limit_requires_release() {
        let registry = UploadStreamRegistry::with_max_retained(1);
        let first = tempfile::NamedTempFile::new().expect("first temp upload");
        registry
            .retain_temp_file(metadata(), first.into_temp_path())
            .expect("first upload retained");

        let second = tempfile::NamedTempFile::new().expect("second temp upload");
        let error = registry
            .retain_temp_file(metadata(), second.into_temp_path())
            .expect_err("second upload should hit cap");
        assert_eq!(error.kind, "RegistryFull");
    }

    fn metadata() -> UploadStreamMetadata {
        UploadStreamMetadata {
            field: "file".to_string(),
            filename: None,
            content_type: None,
            size: 0,
        }
    }
}
