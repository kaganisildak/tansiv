/// Non-thread safe Rust version of Tom Herbert's Linux kernel's dynamic queue limits (dql)
use std::time::Duration;

#[derive(Debug)]
pub struct Dql {
    // Fields accessed in enqueue path (Dql::queued)
    num_queued: usize,          // Total ever queued
    adj_limit: usize,           // limit + num_completed 
    last_obj_count: usize,      // Count at last queuing 

    // Fields accessed only by completion path (Dql::completed)

    limit: usize,               // Current limit
    num_completed: usize,       // Total ever completed

    prev_ovlimit: usize,        // Previous over limit
    prev_num_queued: usize,     // Previous queue total
    prev_last_obj_count: usize, // Previous queuing cnt

    lowest_slack: usize,        // Lowest slack found
    slack_start_time: Duration, // Time slacks seen

    // Configuration
    max_limit: usize,           // Max limit
    min_limit: usize,           // Minimum limit
    slack_hold_time: Duration,  // Time to measure slack
}

impl Dql {
    const MAX_OBJECT: usize = u32::MAX as usize / 16;
    const MAX_LIMIT: usize = u32::MAX as usize / 2 - Self::MAX_OBJECT;
    const LOWEST_SLACK_INIT: usize = u32::MAX as usize;

    pub fn new(now: Duration, hold_time: Duration) -> Dql {
        Dql {
            num_queued: 0,
            adj_limit: 0,
            last_obj_count: 0,
            limit: 0,
            num_completed: 0,
            prev_ovlimit: 0,
            prev_num_queued: 0,
            prev_last_obj_count: 0,
            lowest_slack: Self::LOWEST_SLACK_INIT,
            slack_start_time: now,
            max_limit: Self::MAX_LIMIT,
            min_limit: 0,
            slack_hold_time: hold_time,
        }
    }

    /// Record number of objects queued. Assumes that caller has already checked
    /// availability in the queue with Dql::available.
    pub fn queued(&mut self, count: usize) {
        assert!(count <= Self::MAX_OBJECT);

        self.last_obj_count = count;
        self.num_queued += count;
    }

    /// Returns the current limit of the queue
    pub fn get_limit(&self) -> usize {
        self.limit
    }

    /// Returns how many objects can be queued, None indicates over limit.
    pub fn available(&self) -> Option<usize> {
            self.adj_limit.checked_sub(self.num_queued)
    }

    /// Record number of completed objects and recalculate the limit.
    pub fn completed(&mut self, count: usize, now: Duration) {
        let num_queued = self.num_queued;

        /* Can't complete more than what's in queue */
        assert!(count <= num_queued - self.num_completed);

        let completed = self.num_completed + count;
        let mut limit = self.limit;
        let mut ovlimit = (num_queued - self.num_completed).saturating_sub(limit);
        let inprogress = num_queued != completed;
        let prev_inprogress = self.prev_num_queued != self.num_completed;
        let all_prev_completed = completed >= self.prev_num_queued;

        if (ovlimit > 0 && !inprogress) || (self.prev_ovlimit > 0 && all_prev_completed) {
            /*
             * Queue considered starved if:
             *   - The queue was over-limit in the last interval,
             *     and there is no more data in the queue.
             *  OR
             *   - The queue was over-limit in the previous interval and
             *     when enqueuing it was possible that all queued data
             *     had been consumed.  This covers the case when queue
             *     may have becomes starved between completion processing
             *     running and next time enqueue was scheduled.
             *
             *     When queue is starved increase the limit by the amount
             *     of bytes both sent and completed in the last interval,
             *     plus any previous over-limit.
             */
            limit += completed.saturating_sub(self.prev_num_queued) + self.prev_ovlimit;
            self.slack_start_time = now;
            self.lowest_slack = Self::LOWEST_SLACK_INIT;
        } else if inprogress && prev_inprogress && !all_prev_completed {
            /*
             * Queue was not starved, check if the limit can be decreased.
             * A decrease is only considered if the queue has been busy in
             * the whole interval (the check above).
             *
             * If there is slack, the amount of excess data queued above
             * the amount needed to prevent starvation, the queue limit
             * can be decreased.  To avoid hysteresis we consider the
             * minimum amount of slack found over several iterations of the
             * completion routine.
             */

            /*
             * Slack is the maximum of
             *   - The queue limit plus previous over-limit minus twice
             *     the number of objects completed.  Note that two times
             *     number of completed bytes is a basis for an upper bound
             *     of the limit.
             *   - Portion of objects in the last queuing operation that
             *     was not part of non-zero previous over-limit.  That is
             *     "round down" by non-overlimit portion of the last
             *     queueing operation.
             */
            let slack = (limit + self.prev_ovlimit).saturating_sub(2 * (completed - self.num_completed));
            let slack_last_objs = if self.prev_ovlimit > 0 {
                self.prev_last_obj_count.saturating_sub(self.prev_ovlimit)
            } else {
                0
            };

            let slack = usize::max(slack, slack_last_objs);

            if slack < self.lowest_slack {
                self.lowest_slack = slack;
            }

            if now > self.slack_start_time + self.slack_hold_time {
                limit = limit.saturating_sub(self.lowest_slack);
                self.slack_start_time = now;
                self.lowest_slack = Self::LOWEST_SLACK_INIT;
            }
        }

        /* Enforce bounds on limit */
        let limit = usize::min(usize::max(limit, self.min_limit), self.max_limit);

        if limit != self.limit {
            self.limit = limit;
            ovlimit = 0;
        }

        self.adj_limit = limit + completed;
        self.prev_ovlimit = ovlimit;
        self.prev_last_obj_count = self.last_obj_count;
        self.num_completed = completed;
        self.prev_num_queued = num_queued;
    }

    /// Reset dql state
    pub fn reset(&mut self, now: Duration) {
        self.num_queued = 0;
        self.adj_limit = 0;
        self.last_obj_count = 0;
        self.limit = 0;
        self.num_completed = 0;
        self.prev_ovlimit = 0;
        self.prev_num_queued = 0;
        self.prev_last_obj_count = 0;
        self.lowest_slack = Self::LOWEST_SLACK_INIT;
        self.slack_start_time = now;
    }
}
