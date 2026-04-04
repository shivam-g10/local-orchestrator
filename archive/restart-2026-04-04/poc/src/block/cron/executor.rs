use std::str::FromStr;

use crate::{
    block::{ExecutionResult, ExecutionRunResult, TriggerType},
    logger,
};

use super::CronBlockBody;
use chrono::Utc;
use cron::Schedule;
use crossbeam::channel::bounded;

pub fn execute_cron(_: Option<String>, body: CronBlockBody) -> ExecutionRunResult {
    let (send, rec) = bounded::<Option<String>>(0);
    let schedule = Schedule::from_str(&body.cron).unwrap();

    std::thread::spawn(move || {
        loop {
            let now = Utc::now();
            let next_run = match schedule.after(&now).next() {
                None => {
                    logger::error("Error getting next run time");
                    return Err::<(), ()>(());
                }
                Some(t) => t,
            };
            logger::debug(&format!("Next run at {}", &next_run.to_string()));
            let sleep_time = match (next_run - now).to_std() {
                Err(e) => {
                    logger::error(&format!("Error getting sleep duration: {:#?}", e));
                    return Err(());
                }
                Ok(t) => t,
            };
            logger::debug(&format!("Going to sleep for {}ms", &sleep_time.as_millis()));
            std::thread::sleep(sleep_time);
            let result = send.send(Some(Utc::now().to_string()));
            logger::debug(&format!("result from send: {:#?}", result));
        }
    });
    Ok(Some(ExecutionResult::Trigger(rec, TriggerType::Recurring)))
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_cron_run() {
        let result = execute_cron(
            None,
            CronBlockBody {
                cron: "* * * * * * *".to_owned(),
            },
        );
        assert!(result.is_ok());
        let result = result.unwrap().unwrap();
        let rec = match result {
            ExecutionResult::Trigger(rec, t_type) => {
                assert_eq!(t_type, TriggerType::Recurring);
                rec
            }
            _ => {
                panic!("Got response instead of trigger")
            }
        };

        let receive = rec.recv_timeout(Duration::from_millis(1000));
        assert!(receive.is_ok());
        let msg = receive.unwrap();
        assert!(msg.is_some());
    }
}
