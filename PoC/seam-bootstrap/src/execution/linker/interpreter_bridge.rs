use crate::ast::ResourceId;

use super::PathResult;

/// Instruction parsed from pseudo-code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    ReadResource(u32),
    WriteResource(u32),
    ReadWriteResource(u32),
    Barrier,
    Success,
    Abort,
}

/// Executes pseudo-code generated from compiled fork metadata.
pub struct CodeInterpreter;

impl CodeInterpreter {
    pub fn parse(code: &str) -> Vec<Instruction> {
        let mut instructions = Vec::new();

        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            match parts[0] {
                "read" => {
                    if parts.len() > 1 {
                        if let Ok(id) = parts[1].parse::<u32>() {
                            instructions.push(Instruction::ReadResource(id));
                        }
                    }
                }
                "write" => {
                    if parts.len() > 1 {
                        if let Ok(id) = parts[1].parse::<u32>() {
                            instructions.push(Instruction::WriteResource(id));
                        }
                    }
                }
                "readwrite" => {
                    if parts.len() > 1 {
                        if let Ok(id) = parts[1].parse::<u32>() {
                            instructions.push(Instruction::ReadWriteResource(id));
                        }
                    }
                }
                "barrier" => instructions.push(Instruction::Barrier),
                "success" => instructions.push(Instruction::Success),
                "abort" => instructions.push(Instruction::Abort),
                _ => {}
            }
        }

        instructions
    }

    pub fn execute(
        path_id: u32,
        resource_id: ResourceId,
        instructions: &[Instruction],
    ) -> PathResult {
        let mut result = PathResult::new(path_id, resource_id);
        let mut aborted = false;

        for instruction in instructions {
            match instruction {
                Instruction::ReadResource(_) => {}
                Instruction::WriteResource(_) => {}
                Instruction::ReadWriteResource(_) => {}
                Instruction::Barrier => {
                    std::sync::atomic::fence(std::sync::atomic::Ordering::Release);
                }
                Instruction::Success => {
                    result = result.success();
                }
                Instruction::Abort => {
                    aborted = true;
                    result = result.abort();
                    break;
                }
            }
        }

        if !result.success && !aborted {
            result = result.success();
        }

        result
    }
}

/// Resource access tracker for path execution.
#[derive(Debug, Clone)]
pub struct ResourceAccessTracker {
    pub path_id: u32,
    pub reads: Vec<u32>,
    pub writes: Vec<u32>,
}

impl ResourceAccessTracker {
    pub fn new(path_id: u32) -> Self {
        ResourceAccessTracker {
            path_id,
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    pub fn record_read(&mut self, resource_id: u32) {
        if !self.reads.contains(&resource_id) {
            self.reads.push(resource_id);
        }
    }

    pub fn record_write(&mut self, resource_id: u32) {
        if !self.writes.contains(&resource_id) {
            self.writes.push(resource_id);
        }
    }

    pub fn total_accesses(&self) -> usize {
        self.reads.len() + self.writes.len()
    }
}
