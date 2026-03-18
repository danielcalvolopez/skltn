import { useMemo } from 'react';
import type { UsageRecord, SavingsRecord, DrilldownRecord, ModelCost, SkltnSavings, ContextMetrics } from '../types/usage';
import { calculateSessionTotals, calculateModelBreakdown, calculateSkltnSavings, calculateContextMetrics } from '../types/usage';

export interface SessionMetrics {
    totalCost: number;
    requestCount: number;
    totalTokens: number;
    skltnSavings: SkltnSavings;
    modelBreakdown: ModelCost[];
    contextMetrics: ContextMetrics;
}

export function useSessionMetrics(
    records: UsageRecord[],
    savingsRecords: SavingsRecord[],
    drilldownRecords: DrilldownRecord[],
): SessionMetrics {
    return useMemo(() => {
        const totals = calculateSessionTotals(records);
        const modelBreakdown = calculateModelBreakdown(records);
        const skltnSavings = calculateSkltnSavings(savingsRecords, records);
        const contextMetrics = calculateContextMetrics(savingsRecords, drilldownRecords, records);

        return {
            totalCost: totals.totalCost,
            requestCount: records.length,
            totalTokens: totals.totalTokens,
            skltnSavings,
            modelBreakdown,
            contextMetrics,
        };
    }, [records, savingsRecords, drilldownRecords]);
}
