import ReactECharts from 'echarts-for-react';
import type { EChartsOption } from 'echarts';

interface TokenChartProps {
    chartOptions: EChartsOption;
}

export function TokenChart({ chartOptions }: TokenChartProps) {
    return (
        <div className="token-chart">
            <ReactECharts
                option={chartOptions}
                style={{ height: '100%', width: '100%' }}
                opts={{ renderer: 'canvas' }}
                notMerge={true}
            />
        </div>
    );
}
