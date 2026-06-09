use serde::{Deserialize, Serialize};

use crate::debug::SourceSpan;
use crate::op::Op;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub file: String,
    pub ops: Vec<Op>,
    pub debug: Vec<SourceSpan>,
    pub blocks: Vec<Chunk>,
}

impl Chunk {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            ops: Vec::new(),
            debug: Vec::new(),
            blocks: Vec::new(),
        }
    }

    pub fn push(&mut self, op: Op, span: SourceSpan) {
        self.ops.push(op);
        self.debug.push(span);
    }

    pub fn push_block(&mut self, block: Chunk) -> usize {
        let index = self.blocks.len();
        self.blocks.push(block);
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::Op;

    #[test]
    fn chunk_records_ops_and_source_spans() {
        let mut chunk = Chunk::new("app/Controllers/HomeController.rco");
        let span = SourceSpan {
            file: chunk.file.clone(),
            start: 10,
            end: 15,
            line: 2,
            column: 3,
        };
        chunk.push(Op::PushString("home/index".to_string()), span.clone());
        chunk.push(Op::CallWord("view".to_string()), span.clone());

        assert_eq!(chunk.ops.len(), 2);
        assert_eq!(chunk.debug[0].line, 2);
        assert_eq!(chunk.debug[1].file, "app/Controllers/HomeController.rco");
    }
}
