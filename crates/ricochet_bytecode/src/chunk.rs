use serde::{Deserialize, Serialize};

use crate::debug::SourceSpan;
use crate::op::Op;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instruction {
    pub op: Op,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub file: String,
    pub instructions: Vec<Instruction>,
    pub blocks: Vec<Chunk>,
}

impl Chunk {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            instructions: Vec::new(),
            blocks: Vec::new(),
        }
    }

    pub fn push(&mut self, op: Op, span: SourceSpan) {
        self.instructions.push(Instruction { op, span });
    }

    pub fn push_block(&mut self, block: Chunk) -> usize {
        let index = self.blocks.len();
        self.blocks.push(block);
        index
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn ops(&self) -> impl Iterator<Item = &Op> {
        self.instructions
            .iter()
            .map(|instruction| &instruction.op)
    }

    pub fn debug(&self) -> impl Iterator<Item = &SourceSpan> {
        self.instructions
            .iter()
            .map(|instruction| &instruction.span)
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

        assert_eq!(chunk.instructions.len(), 2);
        assert_eq!(
            chunk.instructions[0].op,
            Op::PushString("home/index".to_string())
        );
        assert_eq!(chunk.instructions[0].span.line, 2);
        assert_eq!(
            chunk.instructions[1].span.file,
            "app/Controllers/HomeController.rco"
        );
        assert_eq!(
            chunk.ops().cloned().collect::<Vec<_>>(),
            vec![
                Op::PushString("home/index".to_string()),
                Op::CallWord("view".to_string())
            ]
        );
        assert_eq!(
            chunk
                .debug()
                .map(|span| (span.line, span.column))
                .collect::<Vec<_>>(),
            vec![(2, 3), (2, 3)]
        );
    }

    #[test]
    fn push_block_returns_nested_block_index_and_stores_chunk() {
        let mut chunk = Chunk::new("app/Controllers/HomeController.rco");
        let block = Chunk::new("app/Views/home/index.rco");

        let index = chunk.push_block(block.clone());

        assert_eq!(index, 0);
        assert_eq!(chunk.blocks[index], block);

        let second_index = chunk.push_block(Chunk::new("app/Views/home/show.rco"));

        assert_eq!(second_index, 1);
    }

    #[test]
    fn serde_roundtrip_preserves_chunk_contents() {
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

        let mut block = Chunk::new("app/Views/home/index.rco");
        block.push(Op::Return, span);
        chunk.push_block(block.clone());

        let json = serde_json::to_string(&chunk).expect("chunk should serialize");
        let decoded: Chunk = serde_json::from_str(&json).expect("chunk should deserialize");

        assert_eq!(decoded.file, chunk.file);
        assert_eq!(decoded.instructions, chunk.instructions);
        assert_eq!(decoded.blocks, vec![block]);
    }

    #[test]
    fn byte_roundtrip_preserves_chunk_contents() {
        let mut chunk = Chunk::new("test.rco");
        let span = SourceSpan {
            file: chunk.file.clone(),
            start: 0,
            end: 5,
            line: 1,
            column: 1,
        };
        chunk.push(Op::PushNumber(2), span.clone());
        chunk.push(Op::PushNumber(3), span.clone());
        chunk.push(Op::CallWord("+".to_string()), span);

        let bytes = chunk.to_bytes().expect("chunk should encode");
        let decoded = Chunk::from_bytes(&bytes).expect("chunk should decode");

        assert_eq!(decoded, chunk);
    }
}
