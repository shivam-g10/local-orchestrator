use crossbeam::channel::Receiver;
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    block::{BlockExecutorTrait, ExecutionResult, TriggerType},
    logger,
};

pub struct Workflow {
    id: Uuid,
    links: HashMap<Uuid, Vec<Uuid>>,
    blocks: HashMap<Uuid, Box<dyn BlockExecutorTrait>>,
}

impl Default for Workflow {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            links: HashMap::new(),
            blocks: HashMap::new(),
        }
    }
}

impl Workflow {
    pub fn new() -> Self {
        Workflow::default()
    }

    pub fn register_block<T: BlockExecutorTrait + 'static>(&mut self, block: T) {
        self.blocks.insert(*block.get_id(), Box::new(block));
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
            value.push(*next.get_id());
            self.links.insert(*prev.get_id(), value);
        }
    }

    pub fn get_id(&self) -> &Uuid {
        &self.id
    }

    pub fn execute(&self, mut next_block: Uuid, start_message: Option<String>) {
        logger::debug(&format!("Executing workflow {}", self.id));
        let mut run = true;
        let mut previous_result: Option<String> = start_message;
        while run {
            let block_result = self.blocks.get(&next_block);
            match block_result {
                None => {
                    run = false;
                    logger::error(&format!("Block {next_block} not found"));
                }
                Some(block) => {
                    logger::info(&format!(
                        "Starting execute on block {} ({})",
                        block.get_block_type(),
                        block.get_id()
                    ));
                    match block.execute(previous_result.clone()) {
                        Err(e) => {
                            logger::error(&format!("Error executing block {next_block} {e}"));
                            run = false;
                        }
                        Ok(result) => {
                            logger::info(&format!(
                                "Completed block {}({next_block}) with result {result:?}",
                                block.get_block_type()
                            ));
                            let mut trigger: Option<Receiver<Option<String>>> = None;
                            let mut trigger_type = TriggerType::Recurring;
                            if let Some(r) = result {
                                match r {
                                    ExecutionResult::Response(r) => {
                                        previous_result = r;
                                    }
                                    ExecutionResult::Trigger(r, t) => {
                                        trigger = Some(r);
                                        trigger_type = t;
                                    }
                                }
                            };

                            if let Some(trig) = trigger {
                                logger::debug("Starting loop");
                                loop {
                                    logger::debug("Loop ITR");
                                    let message = trig.recv();
                                    logger::debug("Got Message");
                                    match message {
                                        Err(e) => {
                                            logger::error(&format!(
                                                "Error in receiving message from trigger: {:#?}",
                                                e
                                            ));
                                            break;
                                        }
                                        Ok(msg) => {
                                            logger::debug(&format!(
                                                "Received message: {:#?}",
                                                &msg
                                            ));
                                            if trigger_type != TriggerType::OneShot {
                                                logger::debug("One shot trigger exiting");
                                                previous_result = msg;
                                                break;
                                            }
                                            let links = match self.links.get(&next_block) {
                                                None => &Vec::new(),
                                                Some(l) => l,
                                            };
                                            let next = links.iter().next();

                                            if let Some(next_id) = next {
                                                self.execute(*next_id, msg);
                                            } else {
                                                logger::info(
                                                    "Exiting trigger loop due to no next item",
                                                );
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            let links = match self.links.get(&next_block) {
                                None => &Vec::new(),
                                Some(l) => l,
                            };

                            let next = links.iter().next();

                            match next {
                                None => {
                                    logger::info(&format!(
                                        "No forward links available from {next_block} stopping run"
                                    ));
                                    run = false;
                                }
                                Some(id) => {
                                    next_block = *id;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
