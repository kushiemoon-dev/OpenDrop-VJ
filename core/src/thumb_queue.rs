//! Port of OpenDrop-VJ `thumbnailer.svelte.ts:47-59`: the pure job-queue
//! logic driving lazy preset-thumbnail rendering. `slot_key` plays the
//! role `slug` played on the web side; here the preset name itself serves
//! as the key, no separate slug system needed on the native side.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbJob {
    pub slot_key: String,
    pub name: String,
}

/// Inserts `job` at the front of `queue`, first dropping any existing job
/// with the same `slot_key`: a re-request for a key jumps back to the
/// front instead of leaving a stale duplicate behind.
pub fn enqueue_front(queue: Vec<ThumbJob>, job: ThumbJob) -> Vec<ThumbJob> {
    let mut out: Vec<ThumbJob> = queue.into_iter().filter(|j| j.slot_key != job.slot_key).collect();
    out.insert(0, job);
    out
}

/// Pops the front job off `queue`. `(None, [])` on an empty queue.
pub fn dequeue_job(mut queue: Vec<ThumbJob>) -> (Option<ThumbJob>, Vec<ThumbJob>) {
    if queue.is_empty() {
        return (None, queue);
    }
    let job = queue.remove(0);
    (Some(job), queue)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod enqueue_front_tests {
        use super::*;

        #[test]
        fn inserts_new_job_at_front() {
            let queue = vec![ThumbJob { slot_key: "a".into(), name: "A".into() }];
            let job = ThumbJob { slot_key: "b".into(), name: "B".into() };
            let result = enqueue_front(queue, job.clone());
            assert_eq!(result[0], job);
            assert_eq!(result.len(), 2);
        }

        #[test]
        fn dedups_by_slot_key_moving_it_to_front() {
            let queue = vec![
                ThumbJob { slot_key: "a".into(), name: "A-old".into() },
                ThumbJob { slot_key: "b".into(), name: "B".into() },
            ];
            let job = ThumbJob { slot_key: "a".into(), name: "A-new".into() };
            let result = enqueue_front(queue, job.clone());
            assert_eq!(result, vec![job, ThumbJob { slot_key: "b".into(), name: "B".into() }]);
        }
    }

    mod dequeue_job_tests {
        use super::*;

        #[test]
        fn on_empty_queue_returns_none_and_empty_queue() {
            let (job, rest) = dequeue_job(Vec::new());
            assert_eq!(job, None);
            assert_eq!(rest, Vec::new());
        }

        #[test]
        fn removes_and_returns_the_first_job() {
            let queue = vec![
                ThumbJob { slot_key: "a".into(), name: "A".into() },
                ThumbJob { slot_key: "b".into(), name: "B".into() },
            ];
            let (job, rest) = dequeue_job(queue);
            assert_eq!(job, Some(ThumbJob { slot_key: "a".into(), name: "A".into() }));
            assert_eq!(rest, vec![ThumbJob { slot_key: "b".into(), name: "B".into() }]);
        }
    }
}
