const express = require('express');
const router = express.Router();
const logger = require('../config/logger');

// Lazy load queue controller to handle Redis unavailability
let queueController;
let dlqController;
try {
  queueController = require('../controllers/queue.controller');
  dlqController = require('../controllers/dlq.controller');
} catch (error) {
  logger.warn('Queue controller unavailable - Redis may not be configured');
}

// Middleware to check if queue system is available
const checkQueueAvailable = (req, res, next) => {
  if (!queueController) {
    return res.status(503).json({
      success: false,
      error: 'Queue system is not available. Please install and configure Redis.',
      documentation: '/api/docs'
    });
  }
  next();
};

/**
 * @swagger
 * /api/queue/stats:
 *   get:
 *     summary: Get queue statistics
 *     tags: [Queue]
 *     responses:
 *       200:
 *         description: Queue statistics retrieved successfully
 *       503:
 *         description: Queue system not available (Redis not configured)
 */
router.get('/stats', checkQueueAvailable, queueController?.getStats || ((req, res) => res.status(503).json({ error: 'Queue service unavailable' })));

/**
 * @swagger
 * /api/queue/jobs:
 *   get:
 *     summary: Get jobs by status
 *     tags: [Queue]
 *     parameters:
 *       - in: query
 *         name: status
 *         schema:
 *           type: string
 *           enum: [waiting, active, completed, failed, delayed]
 *         description: Job status filter
 *       - in: query
 *         name: limit
 *         schema:
 *           type: integer
 *           default: 50
 *         description: Maximum number of jobs to return
 *     responses:
 *       200:
 *         description: Jobs retrieved successfully
 *       503:
 *         description: Queue system not available (Redis not configured)
 */
router.get('/jobs', checkQueueAvailable, queueController?.getJobs || ((req, res) => res.status(503).json({ error: 'Queue service unavailable' })));

/**
 * @swagger
 * /api/queue/clean:
 *   post:
 *     summary: Clean old jobs from queue
 *     tags: [Queue]
 *     responses:
 *       200:
 *         description: Queue cleaned successfully
 *       503:
 *         description: Queue system not available (Redis not configured)
 */
router.post('/clean', checkQueueAvailable, queueController?.clean || ((req, res) => res.status(503).json({ error: 'Queue service unavailable' })));

/**
 * @swagger
 * /api/queue/jobs/{jobId}/retry:
 *   post:
 *     summary: Retry a failed job
 *     tags: [Queue]
 *     parameters:
 *       - in: path
 *         name: jobId
 *         required: true
 *         schema:
 *           type: string
 *         description: Job ID to retry
 *     responses:
 *       200:
 *         description: Job retry initiated
 *       404:
 *         description: Job not found
 *       503:
 *         description: Queue system not available (Redis not configured)
 */
router.post('/jobs/:jobId/retry', checkQueueAvailable, queueController?.retryJob || ((req, res) => res.status(503).json({ error: 'Queue service unavailable' })));

/**
 * @swagger
 * /api/queue/batches/stats:
 *   get:
 *     summary: Get batch processing statistics
 *     tags: [Queue]
 *     responses:
 *       200:
 *         description: Batch statistics retrieved successfully
 */
router.get('/batches/stats', queueController?.getBatchStats || ((req, res) => res.status(503).json({ error: 'Queue service unavailable' })));

/**
 * @swagger
 * /api/queue/batches/flush:
 *   post:
 *     summary: Flush all pending batches immediately
 *     tags: [Queue]
 *     responses:
 *       200:
 *         description: All pending batches flushed successfully
 */
router.post('/batches/flush', queueController?.flushBatches || ((req, res) => res.status(503).json({ error: 'Queue service unavailable' })));

// ─── DLQ (Dead Letter Queue) Routes ──────────────────────────────────────────

const dlqUnavailable = (req, res) => res.status(503).json({ error: 'Queue service unavailable' });

/**
 * @swagger
 * /api/queue/dlq/stats:
 *   get:
 *     summary: Get DLQ statistics across all networks
 *     tags: [DLQ]
 *     responses:
 *       200:
 *         description: DLQ stats per network with failed count and threshold
 *       503:
 *         description: Queue system not available
 */
router.get('/dlq/stats', checkQueueAvailable, dlqController?.getStats || dlqUnavailable);

/**
 * @swagger
 * /api/queue/dlq/jobs:
 *   get:
 *     summary: List failed jobs with fail reasons and stack traces
 *     tags: [DLQ]
 *     parameters:
 *       - in: query
 *         name: network
 *         schema: { type: string, default: testnet }
 *       - in: query
 *         name: start
 *         schema: { type: integer, default: 0 }
 *       - in: query
 *         name: end
 *         schema: { type: integer, default: 49 }
 *     responses:
 *       200:
 *         description: List of failed jobs
 *       503:
 *         description: Queue system not available
 */
router.get('/dlq/jobs', checkQueueAvailable, dlqController?.listFailed || dlqUnavailable);

/**
 * @swagger
 * /api/queue/dlq/jobs/{jobId}:
 *   get:
 *     summary: Get a single failed job with full details
 *     tags: [DLQ]
 *     parameters:
 *       - in: path
 *         name: jobId
 *         required: true
 *         schema: { type: string }
 *       - in: query
 *         name: network
 *         schema: { type: string, default: testnet }
 *     responses:
 *       200:
 *         description: Job details including failedReason and stacktrace
 *       404:
 *         description: Job not found
 *       503:
 *         description: Queue system not available
 */
router.get('/dlq/jobs/:jobId', checkQueueAvailable, dlqController?.getJob || dlqUnavailable);

/**
 * @swagger
 * /api/queue/dlq/jobs/{jobId}/replay:
 *   post:
 *     summary: Replay (retry) a specific failed job
 *     tags: [DLQ]
 *     parameters:
 *       - in: path
 *         name: jobId
 *         required: true
 *         schema: { type: string }
 *       - in: query
 *         name: network
 *         schema: { type: string, default: testnet }
 *     responses:
 *       200:
 *         description: Job replayed successfully
 *       404:
 *         description: Job not found
 *       503:
 *         description: Queue system not available
 */
router.post('/dlq/jobs/:jobId/replay', checkQueueAvailable, dlqController?.replayJob || dlqUnavailable);

/**
 * @swagger
 * /api/queue/dlq/jobs/{jobId}:
 *   delete:
 *     summary: Remove a specific failed job
 *     tags: [DLQ]
 *     parameters:
 *       - in: path
 *         name: jobId
 *         required: true
 *         schema: { type: string }
 *       - in: query
 *         name: network
 *         schema: { type: string, default: testnet }
 *     responses:
 *       200:
 *         description: Job removed
 *       404:
 *         description: Job not found
 *       503:
 *         description: Queue system not available
 */
router.delete('/dlq/jobs/:jobId', checkQueueAvailable, dlqController?.removeJob || dlqUnavailable);

/**
 * @swagger
 * /api/queue/dlq/replay-all:
 *   post:
 *     summary: Replay all failed jobs in a network queue
 *     tags: [DLQ]
 *     parameters:
 *       - in: query
 *         name: network
 *         schema: { type: string, default: testnet }
 *     responses:
 *       200:
 *         description: All failed jobs replayed
 *       503:
 *         description: Queue system not available
 */
router.post('/dlq/replay-all', checkQueueAvailable, dlqController?.replayAll || dlqUnavailable);

/**
 * @swagger
 * /api/queue/dlq/clear:
 *   delete:
 *     summary: Bulk-clear all failed jobs in a network queue
 *     tags: [DLQ]
 *     parameters:
 *       - in: query
 *         name: network
 *         schema: { type: string, default: testnet }
 *     responses:
 *       200:
 *         description: All failed jobs cleared
 *       503:
 *         description: Queue system not available
 */
router.delete('/dlq/clear', checkQueueAvailable, dlqController?.clearAll || dlqUnavailable);

module.exports = router;
