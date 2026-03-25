const { enqueueAction, getQueueStats } = require('../src/worker/queue');

describe('Queue Integration', () => {
    test('should enqueue an action job', async () => {
        const trigger = {
            _id: 'test-trigger-123',
            actionType: 'webhook',
            actionUrl: 'https://example.com/webhook',
            contractId: 'CTEST123',
            eventName: 'transfer',
        };

        const eventPayload = {
            from: 'GTEST123',
            to: 'GTEST456',
            amount: '1000',
        };

        const job = await enqueueAction(trigger, eventPayload);

        expect(job).toBeDefined();
        expect(job.id).toBeDefined();
        expect(job.data.trigger).toEqual(trigger);
        expect(job.data.eventPayload).toEqual(eventPayload);
    });

    test('should get queue statistics', async () => {
        const stats = await getQueueStats();

        expect(stats).toBeDefined();
        expect(stats).toHaveProperty('waiting');
        expect(stats).toHaveProperty('active');
        expect(stats).toHaveProperty('completed');
        expect(stats).toHaveProperty('failed');
        expect(stats).toHaveProperty('delayed');
        expect(stats).toHaveProperty('total');
    });
});
