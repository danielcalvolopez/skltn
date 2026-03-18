export interface UsageRecord {
    timestamp: string;
    model: string;
    input_tokens: number;
    output_tokens: number;
    cache_creation_input_tokens: number;
    cache_read_input_tokens: number;
    cost_usd: number;
}

// Duplicated from skltn-obs pricing.rs — update both when prices change
const MODEL_RATES: Record<string, { input: number; cacheRead: number }> = {
    'claude-opus-4': { input: 15.0, cacheRead: 1.5 },
    'claude-sonnet-4': { input: 3.0, cacheRead: 0.3 },
    'claude-haiku-4': { input: 0.8, cacheRead: 0.08 },
    'claude-3-7-sonnet': { input: 3.0, cacheRead: 0.3 },
    'claude-3-5-sonnet': { input: 3.0, cacheRead: 0.3 },
    'claude-3-5-haiku': { input: 0.8, cacheRead: 0.08 },
};

export const getCacheSavingsRate = (
    model: string,
): { input: number; cacheRead: number } => {
    const entry = Object.entries(MODEL_RATES).find(([key]) =>
        model.includes(key),
    );
    return entry ? entry[1] : { input: 0, cacheRead: 0 };
};

export const calculateCacheSavings = (record: UsageRecord): number => {
    const rates = getCacheSavingsRate(record.model);
    return (
        (record.cache_read_input_tokens * (rates.input - rates.cacheRead)) /
        1_000_000
    );
};

export interface SessionTotals {
    totalCost: number;
    totalTokens: number;
    cacheSavings: number;
    totalCacheRead: number;
    totalInput: number;
}

export const calculateSessionTotals = (
    records: UsageRecord[],
): SessionTotals => {
    return records.reduce<SessionTotals>(
        (acc, record) => ({
            totalCost: acc.totalCost + record.cost_usd,
            totalTokens:
                acc.totalTokens +
                record.input_tokens +
                record.output_tokens +
                record.cache_read_input_tokens +
                record.cache_creation_input_tokens,
            cacheSavings: acc.cacheSavings + calculateCacheSavings(record),
            totalCacheRead:
                acc.totalCacheRead + record.cache_read_input_tokens,
            totalInput:
                acc.totalInput +
                record.input_tokens +
                record.cache_read_input_tokens +
                record.cache_creation_input_tokens,
        }),
        {
            totalCost: 0,
            totalTokens: 0,
            cacheSavings: 0,
            totalCacheRead: 0,
            totalInput: 0,
        },
    );
};

export interface ModelCost {
    model: string;
    cost: number;
}

export const calculateModelBreakdown = (
    records: UsageRecord[],
): ModelCost[] => {
    const costMap = new Map<string, number>();
    for (const record of records) {
        costMap.set(
            record.model,
            (costMap.get(record.model) ?? 0) + record.cost_usd,
        );
    }
    return Array.from(costMap.entries())
        .map(([model, cost]) => ({ model, cost }))
        .sort((a, b) => b.cost - a.cost);
};
