export interface UsageRecord {
    timestamp: string;
    model: string;
    input_tokens: number;
    output_tokens: number;
    cache_creation_input_tokens: number;
    cache_read_input_tokens: number;
    cost_usd: number;
}

export interface SavingsRecord {
    timestamp: string;
    file: string;
    language: string;
    original_tokens: number;
    skeleton_tokens: number;
    saved_tokens: number;
}

export interface DrilldownRecord {
    timestamp: string;
    file: string;
    symbol: string;
    tokens: number;
}

// Duplicated from skltn-obs pricing.rs — update both when prices change
const MODEL_RATES: Record<string, { input: number }> = {
    'claude-opus-4': { input: 15.0 },
    'claude-sonnet-4': { input: 3.0 },
    'claude-haiku-4': { input: 0.8 },
    'claude-3-7-sonnet': { input: 3.0 },
    'claude-3-5-sonnet': { input: 3.0 },
    'claude-3-5-haiku': { input: 0.8 },
};

const MODEL_CONTEXT_LIMITS: Record<string, number> = {
    'claude-opus-4': 1_000_000,
    'claude-sonnet-4': 200_000,
    'claude-haiku-4': 200_000,
    'claude-3-7-sonnet': 200_000,
    'claude-3-5-sonnet': 200_000,
    'claude-3-5-haiku': 200_000,
};

export const getModelContextLimit = (model: string): number => {
    const entry = Object.entries(MODEL_CONTEXT_LIMITS).find(([key]) =>
        model.includes(key),
    );
    return entry ? entry[1] : 200_000;
};

export const getModelInputRate = (model: string): number => {
    const entry = Object.entries(MODEL_RATES).find(([key]) =>
        model.includes(key),
    );
    return entry ? entry[1].input : 0;
};

export interface SessionTotals {
    totalCost: number;
    totalTokens: number;
    totalInput: number;
    totalOutput: number;
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
            totalInput:
                acc.totalInput +
                record.input_tokens +
                record.cache_read_input_tokens +
                record.cache_creation_input_tokens,
            totalOutput: acc.totalOutput + record.output_tokens,
        }),
        {
            totalCost: 0,
            totalTokens: 0,
            totalInput: 0,
            totalOutput: 0,
        },
    );
};

export interface SkltnSavings {
    savedTokens: number;
    originalTokens: number;
    skeletonTokens: number;
    savingsUsd: number;
    savingsRatio: number;
}

/**
 * Compute skltn savings in USD using the dominant model's input rate.
 * The dominant model is the one with the most input tokens in the session.
 */
export const calculateSkltnSavings = (
    savingsRecords: SavingsRecord[],
    usageRecords: UsageRecord[],
): SkltnSavings => {
    const savedTokens = savingsRecords.reduce((sum, r) => sum + r.saved_tokens, 0);
    const originalTokens = savingsRecords.reduce((sum, r) => sum + r.original_tokens, 0);
    const skeletonTokens = savingsRecords.reduce((sum, r) => sum + r.skeleton_tokens, 0);

    // Find dominant model by total input tokens
    const modelInputs = new Map<string, number>();
    for (const record of usageRecords) {
        const total = record.input_tokens + record.cache_read_input_tokens + record.cache_creation_input_tokens;
        modelInputs.set(record.model, (modelInputs.get(record.model) ?? 0) + total);
    }

    let dominantRate = 0;
    let maxInput = 0;
    for (const [model, total] of modelInputs) {
        if (total > maxInput) {
            maxInput = total;
            dominantRate = getModelInputRate(model);
        }
    }

    const savingsUsd = (savedTokens * dominantRate) / 1_000_000;
    const savingsRatio = originalTokens > 0 ? savedTokens / originalTokens : 0;

    return { savedTokens, originalTokens, skeletonTokens, savingsUsd, savingsRatio };
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

export interface ContextMetrics {
    explorationMultiplier: number;
    filesExplored: number;
    contextDensity: number;
    drilldownCount: number;
    contextLimit: number;
}

export const calculateContextMetrics = (
    savingsRecords: SavingsRecord[],
    drilldownRecords: DrilldownRecord[],
    usageRecords: UsageRecord[],
): ContextMetrics => {
    const originalTokens = savingsRecords.reduce((sum, r) => sum + r.original_tokens, 0);
    const skeletonTokens = savingsRecords.reduce((sum, r) => sum + r.skeleton_tokens, 0);

    const explorationMultiplier = skeletonTokens > 0 ? originalTokens / skeletonTokens : 1;

    // Unique files from savings + drilldown records
    const files = new Set<string>();
    for (const r of savingsRecords) files.add(r.file);
    for (const r of drilldownRecords) files.add(r.file);

    const contextDensity = originalTokens > 0 ? skeletonTokens / originalTokens : 0;

    // Find dominant model for context limit
    const modelInputs = new Map<string, number>();
    for (const record of usageRecords) {
        const total = record.input_tokens + record.cache_read_input_tokens + record.cache_creation_input_tokens;
        modelInputs.set(record.model, (modelInputs.get(record.model) ?? 0) + total);
    }
    let dominantModel = '';
    let maxInput = 0;
    for (const [model, total] of modelInputs) {
        if (total > maxInput) {
            maxInput = total;
            dominantModel = model;
        }
    }
    const contextLimit = dominantModel ? getModelContextLimit(dominantModel) : 200_000;

    return {
        explorationMultiplier,
        filesExplored: files.size,
        contextDensity,
        drilldownCount: drilldownRecords.length,
        contextLimit,
    };
};
