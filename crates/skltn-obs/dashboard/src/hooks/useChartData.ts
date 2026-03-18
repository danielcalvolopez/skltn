import { useMemo } from 'react';
import type { EChartsOption } from 'echarts';
import type { UsageRecord, SavingsRecord } from '../types/usage';

export function useChartData(
    records: UsageRecord[],
    savingsRecords: SavingsRecord[],
    contextLimit: number,
): EChartsOption {
    return useMemo(() => {
        // Build a map of cumulative saved tokens by timestamp
        // so we can show "without skltn" as actual + saved
        const savedByTime = new Map<string, number>();
        let cumulativeSaved = 0;
        const sortedSavings = [...savingsRecords].sort(
            (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime(),
        );
        for (const s of sortedSavings) {
            cumulativeSaved += s.saved_tokens;
            savedByTime.set(s.timestamp, cumulativeSaved);
        }

        // Build cumulative actual tokens per request
        let cumulativeActual = 0;
        let lastSaved = 0;
        const actualData: [string, number][] = [];
        const withoutSkltnData: [string, number][] = [];

        for (const r of records) {
            const tokens = r.input_tokens + r.output_tokens +
                r.cache_read_input_tokens + r.cache_creation_input_tokens;
            cumulativeActual += tokens;

            // Find the most recent saved total at or before this timestamp
            const rTime = new Date(r.timestamp).getTime();
            for (const s of sortedSavings) {
                if (new Date(s.timestamp).getTime() <= rTime) {
                    lastSaved = savedByTime.get(s.timestamp) ?? lastSaved;
                }
            }

            actualData.push([r.timestamp, cumulativeActual]);
            withoutSkltnData.push([r.timestamp, cumulativeActual + lastSaved]);
        }

        return {
            backgroundColor: 'transparent',
            grid: { top: 30, right: 12, bottom: 30, left: 60 },
            tooltip: {
                trigger: 'axis',
                backgroundColor: '#1a1a1a',
                borderColor: '#333',
                textStyle: {
                    color: '#ccc',
                    fontFamily: 'JetBrains Mono',
                    fontSize: 10,
                },
            },
            legend: {
                top: 4,
                right: 12,
                textStyle: {
                    color: '#555',
                    fontFamily: 'JetBrains Mono',
                    fontSize: 9,
                },
            },
            xAxis: {
                type: 'time',
                axisLine: { lineStyle: { color: '#222' } },
                axisLabel: {
                    color: '#444',
                    fontFamily: 'JetBrains Mono',
                    fontSize: 9,
                },
            },
            yAxis: {
                type: 'value',
                name: 'Context Window Tokens',
                nameTextStyle: {
                    color: '#444',
                    fontFamily: 'JetBrains Mono',
                    fontSize: 9,
                },
                splitLine: { lineStyle: { color: '#1a1a1a' } },
                axisLabel: {
                    color: '#444',
                    fontFamily: 'JetBrains Mono',
                    fontSize: 9,
                    formatter: (value: number) => {
                        if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
                        if (value >= 1_000) return `${(value / 1_000).toFixed(0)}K`;
                        return String(value);
                    },
                },
            },
            series: [
                {
                    name: 'Without Skltn',
                    type: 'line',
                    step: 'end',
                    showSymbol: false,
                    data: withoutSkltnData,
                    lineStyle: {
                        width: 1.5,
                        color: '#663333',
                        type: 'dashed',
                    },
                    areaStyle: {
                        color: 'rgba(102, 51, 51, 0.08)',
                    },
                },
                {
                    name: 'With Skltn',
                    type: 'line',
                    step: 'end',
                    showSymbol: false,
                    data: actualData,
                    lineStyle: {
                        width: 2,
                        color: '#00ff88',
                        shadowBlur: 6,
                        shadowColor: 'rgba(0,255,136,0.3)',
                    },
                    areaStyle: {
                        color: 'rgba(0, 255, 136, 0.05)',
                    },
                    markLine: {
                        silent: true,
                        symbol: 'none',
                        lineStyle: { color: '#333', type: 'dashed', width: 1 },
                        label: {
                            formatter: `Context limit`,
                            color: '#444',
                            fontSize: 9,
                            fontFamily: 'JetBrains Mono',
                        },
                        data: [{ yAxis: contextLimit }],
                    },
                },
            ],
        };
    }, [records, savingsRecords, contextLimit]);
}
