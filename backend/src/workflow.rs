use std::collections::HashMap;

use uuid::Uuid;

use crate::block::BlockExecutorTrait;

pub struct Workflow {
    id: Uuid,
    links: HashMap<Uuid, Vec<Uuid>>,
    blocks: HashMap<Uuid, Box<dyn BlockExecutorTrait>>,
}

impl Workflow {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            links: HashMap::new(),
            blocks: HashMap::new(),
        }
    }

    pub fn register_block<T: BlockExecutorTrait + 'static>(&mut self, block: T) {
        self.blocks.insert(block.get_id().clone(), Box::new(block));
    }

    pub fn register_forward_link<T: BlockExecutorTrait, U: BlockExecutorTrait>(
        &mut self,
        prev: &T,
        next: &U,
    ) {
        let mut value = match self.links.get(prev.get_id()) {
            None => Vec::new(),
            Some(list) => list.clone(),
        };
        if !value.contains(next.get_id()) {
            value.push(next.get_id().clone());
            self.links.insert(prev.get_id().clone(), value);
        }
    }

    pub fn get_id(&self) -> &Uuid {
        return &self.id;
    }

    pub fn execute(&self, mut next_block: Uuid) {
        tracing::debug!("Executing workflow {}", self.id);
        let mut run = true;
        let mut previous_result: Option<String> = None;
        while run {
            let block_result = self.blocks.get(&next_block);
            match block_result {
                None => {
                    run = false;
                    tracing::error!("Block {next_block} not found");
                }
                Some(block) => match block.execute(previous_result.clone()) {
                    Err(e) => {
                        tracing::error!("Error executing block {next_block} {e}");
                        run = false;
                    }
                    Ok(result) => {
                        previous_result = result.clone();
                        tracing::info!("Completed block {next_block} with result {result:?}");
                        let links = match self.links.get(&next_block) {
                            None => &Vec::new(),
                            Some(l) => l,
                        };

                        let next = links.iter().next();

                        match next {
                            None => {
                                tracing::info!(
                                    "No forward links available from {next_block} stopping run"
                                );
                                run = false;
                            }
                            Some(id) => {
                                next_block = id.clone();
                            }
                        }
                    }
                },
            }
        }
    }
}
