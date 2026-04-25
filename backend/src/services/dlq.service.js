const axios = require('axios');
const { queues } = require('../worker/queue');
const logger = require('../config/logger');

const DLQ_ALERT_THRESHOLD = Number(process.env.DLQ_ALERT_THRESHOLD || 10);
const DLQ_SLACK_WEBHOOK_URL = process.env.DLQ_SLACK_WEBHOOK_URL;
const DLQ_DISCORD_WEBHOOK_URL = process.env.DLQ_DISCORD_WEBHOOK_URL;

/**
 * Get all failed jobs across all network queues, with fail reason and stack trace.
 */
async function getFailedJobs(network, { start = 0, end = 99 } = {}) {
    const results = {};

    const targets = network
        ? { [network]: queues[network] }
        : queues;

    for (const [net, queue] of Object.entries(targets)) {
        if (!queue) continue;
        const jobs = await queue.getFailed(start, end);
        results[net] = jobs.map(job => ({
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

    return results;
}

/**
 * Replay (retry) a single failed job by ID.
 */
async function replayJob(network, jobId) {
    const queue = queues[network];
    if (!queue) throw new Error(`Queue for network '${network}' not found`);

    const job = await queue.getJob(jobId);
    if (!job) throw new Error(`Job '${jobId}' not found in network '${network}'`);

    await job.retry('failed');
    logger.info('DLQ: job replayed', { network, jobId });
    return { jobId, network };
}

/**
 * Replay all failed jobs in a network queue.
 */
async function replayAllFailed(network) {
    const queue = queues[network];
    if (!queue) throw new Error(`Queue for network '${network}' not found`);

    const jobs = await queue.getFailed(0, -1);
    await Promise.all(jobs.map(job => job.retry('failed')));
    logger.info('DLQ: all failed jobs replayed', { network, count: jobs.length });
    return { network, replayed: jobs.length };
}

/**
 * Remove (clear) specific failed jobs by IDs.
 */
async function clearJobs(network, jobIds) {
    const queue = queues[network];
    if (!queue) throw new Error(`Queue for network '${network}' not found`);

    let removed = 0;
    for (const jobId of jobIds) {
        const job = await queue.getJob(jobId);
        if (job) {
            await job.remove();
            removed++;
        }
    }
    logger.info('DLQ: jobs cleared', { network, removed });
    return { network, removed };
}

/**
 * Remove all failed jobs in a network queue.
 */
async function clearAllFailed(network) {
    const queue = queues[network];
    if (!queue) throw new Error(`Queue for network '${network}' not found`);

    const jobs = await queue.getFailed(0, -1);
    await Promise.all(jobs.map(job => job.remove()));
    logger.info('DLQ: all failed jobs cleared', { network, count: jobs.length });
    return { network, removed: jobs.length };
}

/**
 * Check failed job count against threshold and send alerts if exceeded.
 */
async function checkThresholdAndAlert(network, queue) {
    try {
        const counts = await queue.getJobCounts('failed');
        const failedCount = counts.failed || 0;

        if (failedCount < DLQ_ALERT_THRESHOLD) return;

        logger.warn('DLQ threshold exceeded', { network, failedCount, threshold: DLQ_ALERT_THRESHOLD });

        const message = `🚨 *DLQ Alert* — \`${network}\` queue has *${failedCount}* failed jobs (threshold: ${DLQ_ALERT_THRESHOLD})`;

        await Promise.allSettled([
            DLQ_SLACK_WEBHOOK_URL && sendSlackAlert(message),
            DLQ_DISCORD_WEBHOOK_URL && sendDiscordAlert(message, network, failedCount),
        ]);
    } catch (err) {
        logger.error('DLQ threshold check failed', { error: err.message });
    }
}

async function sendSlackAlert(text) {
    await axios.post(DLQ_SLACK_WEBHOOK_URL, { text });
}

async function sendDiscordAlert(content, network, failedCount) {
    await axios.post(DLQ_DISCORD_WEBHOOK_URL, {
        embeds: [{
            title: '🚨 DLQ Threshold Exceeded',
            description: `Queue \`${network}\` has **${failedCount}** failed jobs`,
            color: 0xFF0000,
            timestamp: new Date().toISOString(),
        }],
    });
}

module.exports = {
    getFailedJobs,
    replayJob,
    replayAllFailed,
    clearJobs,
    clearAllFailed,
    checkThresholdAndAlert,
};
