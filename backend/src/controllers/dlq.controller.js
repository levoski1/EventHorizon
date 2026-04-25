const dlqService = require('../services/dlq.service');
const logger = require('../config/logger');

async function getStats(req, res) {
    try {
        const stats = await dlqService.getStats();
        res.json({ success: true, data: stats });
    } catch (err) {
        logger.error('DLQ getStats failed', { error: err.message });
        res.status(500).json({ success: false, error: err.message });
    }
}

async function listFailed(req, res) {
    try {
        const { network = 'testnet', start = 0, end = 49 } = req.query;
        const jobs = await dlqService.listFailed(network, Number(start), Number(end));
        res.json({ success: true, data: { count: jobs.length, jobs } });
    } catch (err) {
        logger.error('DLQ listFailed failed', { error: err.message });
        res.status(500).json({ success: false, error: err.message });
    }
}

async function getJob(req, res) {
    try {
        const { jobId } = req.params;
        const { network = 'testnet' } = req.query;
        const job = await dlqService.getFailedJob(network, jobId);
        if (!job) return res.status(404).json({ success: false, error: 'Job not found' });
        res.json({ success: true, data: job });
    } catch (err) {
        logger.error('DLQ getJob failed', { error: err.message });
        res.status(500).json({ success: false, error: err.message });
    }
}

async function replayJob(req, res) {
    try {
        const { jobId } = req.params;
        const { network = 'testnet' } = req.query;
        const result = await dlqService.replayJob(network, jobId);
        res.json({ success: true, message: 'Job replayed', data: result });
    } catch (err) {
        logger.error('DLQ replayJob failed', { error: err.message });
        const status = err.message.includes('not found') ? 404 : 500;
        res.status(status).json({ success: false, error: err.message });
    }
}

async function replayAll(req, res) {
    try {
        const { network = 'testnet' } = req.query;
        const result = await dlqService.replayAll(network);
        res.json({ success: true, message: 'All failed jobs replayed', data: result });
    } catch (err) {
        logger.error('DLQ replayAll failed', { error: err.message });
        res.status(500).json({ success: false, error: err.message });
    }
}

async function removeJob(req, res) {
    try {
        const { jobId } = req.params;
        const { network = 'testnet' } = req.query;
        const result = await dlqService.removeJob(network, jobId);
        res.json({ success: true, message: 'Job removed', data: result });
    } catch (err) {
        logger.error('DLQ removeJob failed', { error: err.message });
        const status = err.message.includes('not found') ? 404 : 500;
        res.status(status).json({ success: false, error: err.message });
    }
}

async function clearAll(req, res) {
    try {
        const { network = 'testnet' } = req.query;
        const result = await dlqService.clearAll(network);
        res.json({ success: true, message: 'All failed jobs cleared', data: result });
    } catch (err) {
        logger.error('DLQ clearAll failed', { error: err.message });
        res.status(500).json({ success: false, error: err.message });
    }
}

module.exports = { getStats, listFailed, getJob, replayJob, replayAll, removeJob, clearAll };
