import { useMemo } from 'react';
import type { UsageRecord, ModelCost } from '../types/usage';
import { calculateSessionTotals, calculateModelBreakdown } from '../types/usage';

interface SessionMetrics {
    totalCost: number;
    cacheSavings: number;
    requestCount: number;
    totalTokens: number;
    cacheHitRatio: number;
    modelBreakdown: ModelCost[];
}

export function useSessionMetrics(records: UsageRecord[]): SessionMetrics {
    return useMemo(() => {
        const totals = calculateSessionTotals(records);
        const modelBreakdown = calculateModelBreakdown(records);
        const cacheHitRatio =
            totals.totalInput > 0
                ? totals.totalCacheRead / totals.totalInput
                : 0;

        return {
            totalCost: totals.totalCost,
            cacheSavings: totals.cacheSavings,
            requestCount: records.length,
            totalTokens: totals.totalTokens,
            cacheHitRatio,
            modelBreakdown,
        };
    }, [records]);
}
