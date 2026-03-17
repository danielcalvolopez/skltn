import { useMemo } from 'react';
import type { EChartsOption } from 'echarts';
import type { UsageRecord } from '../types/usage';

export function useChartData(records: UsageRecord[]): EChartsOption {
    return useMemo(() => {
        const inputData = records.map((r) => [r.timestamp, r.input_tokens]);
        const outputData = records.map((r) => [r.timestamp, r.output_tokens]);
        const cacheReadData = records.map((r) => [
            r.timestamp,
            r.cache_read_input_tokens,
        ]);

        return {
            backgroundColor: 'transparent',
            grid: { top: 30, right: 12, bottom: 30, left: 50 },
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
                splitLine: { lineStyle: { color: '#1a1a1a' } },
                axisLabel: {
                    color: '#444',
                    fontFamily: 'JetBrains Mono',
                    fontSize: 9,
                },
            },
            series: [
                {
                    name: 'Input',
                    type: 'line',
                    step: 'end',
                    showSymbol: false,
                    data: inputData,
                    lineStyle: {
                        width: 1.5,
                        color: '#00ff88',
                        shadowBlur: 6,
                        shadowColor: 'rgba(0,255,136,0.3)',
                    },
                },
                {
                    name: 'Output',
                    type: 'line',
                    step: 'end',
                    showSymbol: false,
                    data: outputData,
                    lineStyle: {
                        width: 1.5,
                        color: '#008844',
                        shadowBlur: 4,
                        shadowColor: 'rgba(0,136,68,0.2)',
                    },
                },
                {
                    name: 'Cache Read',
                    type: 'line',
                    step: 'end',
                    showSymbol: false,
                    data: cacheReadData,
                    lineStyle: {
                        width: 1.5,
                        color: '#003322',
                        shadowBlur: 2,
                        shadowColor: 'rgba(0,51,34,0.15)',
                    },
                },
            ],
        };
    }, [records]);
}
