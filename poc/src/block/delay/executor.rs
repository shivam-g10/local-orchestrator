use std::time::Duration;

use crate::{block::{ExecutionResult, ExecutionRunResult, TriggerType}, logger};

use super::DelayBlockBody;
use crossbeam::channel::bounded;


pub fn execute_delay(input: Option<String>, body: DelayBlockBody) -> ExecutionRunResult {
    let (send, rec) = bounded::<Option<String>>(0);
    
    std::thread::spawn(move || {
        loop {
            let sleep_time = Duration::from_millis(body.delay_ms);
            logger::debug(&format!("Going to sleep for {}ms", &sleep_time.as_millis()));
            std::thread::sleep(sleep_time);
            let message = if body.forward_message {
                input.clone()
            } else {
                None
            };
            let result = send.send(message);
            logger::debug(&format!("result from send: {:#?}", result));
        }
        
    });
    Ok(Some(ExecutionResult::Trigger(rec, TriggerType::OneShot)))
}


#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::block::{ExecutionResult, TriggerType};

    use super::*;

    #[test]
    fn test_delay_run() {
        let body = DelayBlockBody {
            delay_ms: 1000,
            forward_message: false,
        };
        let result = execute_delay(None, body);
        assert!(result.is_ok());
        let result = result.unwrap().unwrap();
        let rec = match result {
            ExecutionResult::Trigger(rec, t_type) => {
                assert_eq!(t_type, TriggerType::OneShot);
                rec
            }
            _ => {
                panic!("Got response instead of trigger")
            }
        };
        
        let receive = rec.recv_timeout(Duration::from_millis(1000));
        assert!(receive.is_ok());
        let msg = receive.unwrap();
        assert!(msg.is_none());
    }
}
