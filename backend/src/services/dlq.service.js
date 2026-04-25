const axios = require('axios');
const logger = require('../config/logger');
const { getActionQueue, queues } = require('../worker/queue');

const DLQ_THRESHOLD = Number(process.env.DLQ_ALERT_THRESHOLD || 10);
const DLQ_ALERT_WEBHOOK = process.env.DLQ_ALERT_WEBHOOK_URL || '';
const DLQ_ALERT_TYPE = (process.env.DLQ_ALERT_TYPE || 'discord').toLowerCase(); // 'discord' | 'slack'

/**
 * Send a threshold alert to Discord or Slack.
 */
async function sendThresholdAlert(network, failedCount) {
    if (!DLQ_ALERT_WEBHOOK) return;

    const text = `🚨 *DLQ Alert* — \`${network}\` queue has *${failedCount}* failed jobs (threshold: ${DLQ_THRESHOLD})`;

    try {
        if (DLQ_ALERT_TYPE === 'slack') {
            await axios.post(DLQ_ALERT_WEBHOOK, { text });
        } else {
            // Discord webhook format
            await axios.post(DLQ_ALERT_WEBHOOK, {
                embeds: [{
                    title: '🚨 DLQ Threshold Exceeded',
                    description: text,
                    color: 0xFF0000,
                    fields: [
                        { name: 'Network', value: network, inline: true },
                        { name: 'Failed Jobs', value: String(failedCount), inline: true },
                        { name: 'Threshold', value: String(DLQ_THRESHOLD), inline: true },
                    ],
                    timestamp: new Date().toISOString(),
                }],
            });
        }
        logger.info('DLQ threshold alert sent', { network, failedCount });
    } catch (err) {
        logger.error('Failed to send DLQ alert', { error: err.message });
    }
}

/**
 * Check failed count against threshold and alert if exceeded.
 * Called from the worker's 'failed' event.
 */
async function checkThreshold(network) {
    try {
        const queue = getActionQueue(network);
        const failedCount = await queue.getFailedCount();
        if (failedCount >= DLQ_THRESHOLD) {
            await sendThresholdAlert(network, failedCount);
        }
    } catch (err) {
        logger.error('DLQ threshold check failed', { network, error: err.message });
    }
}

/**
 * List failed jobs with fail reason and stack trace.
 */
async function listFailed(network = 'testnet', start = 0, end = 49) {
    const queue = getActionQueue(network);
    const jobs = await queue.getFailed(start, end);
    return jobs.map(job => ({
        id: job.id,
        name: job.name,
        data: job.data,
        failedReason: job.failedReason,
        stacktrace: job.stacktrace,
        attemptsMade: job.attemptsMade,
        timestamp: job.timestamp,
        finishedOn: job.finishedOn,
    }));
}

/**
 * Get a single failed job by ID.
 */
async function getFailedJob(network, jobId) {
    const queue = getActionQueue(network);
    const job = await queue.getJob(jobId);
    if (!job) return null;
    return {
        id: job.id,
        name: job.name,
        data: job.data,
        failedReason: job.failedReason,
        stacktrace: job.stacktrace,
        attemptsMade: job.attemptsMade,
        timestamp: job.timestamp,
        finishedOn: job.finishedOn,
        opts: job.opts,
    };
}

/**
 * Replay (retry) a single failed job.
 */
async function replayJob(network, jobId) {
    const queue = getActionQueue(network);
    const job = await queue.getJob(jobId);
    if (!job) throw new Error(`Job ${jobId} not found`);
    await job.retry('failed');
    logger.info('DLQ job replayed', { network, jobId });
    return { jobId };
}

/**
 * Replay all failed jobs in a network queue.
 */
async function replayAll(network) {
    const queue = getActionQueue(network);
    const jobs = await queue.getFailed(0, -1);
    await Promise.all(jobs.map(j => j.retry('failed')));
    logger.info('DLQ all jobs replayed', { network, count: jobs.length });
    return { replayed: jobs.length };
}

/**
 * Remove a single failed job.
 */
async function removeJob(network, jobId) {
    const queue = getActionQueue(network);
    const job = await queue.getJob(jobId);
    if (!job) throw new Error(`Job ${jobId} not found`);
    await job.remove();
    logger.info('DLQ job removed', { network, jobId });
    return { jobId };
}

/**
 * Bulk-clear all failed jobs in a network queue.
 */
async function clearAll(network) {
    const queue = getActionQueue(network);
    // clean(grace, limit, type) — 0ms grace removes all
    const removed = await queue.clean(0, 0, 'failed');
    logger.info('DLQ bulk cleared', { network, removed: removed.length });
    return { removed: removed.length };
}

/**
 * DLQ stats across all networks.
 */
async function getStats() {
    const stats = {};
    for (const [network, queue] of Object.entries(queues)) {
        stats[network] = {
            failed: await queue.getFailedCount(),
            threshold: DLQ_THRESHOLD,
        };
    }
    return stats;
}

module.exports = {
    checkThreshold,
    listFailed,
    getFailedJob,
    replayJob,
    replayAll,
    removeJob,
    clearAll,
    getStats,
};
