import type { ContextMetrics } from '../types/usage';

interface MetricsBarProps {
    contextMetrics: ContextMetrics;
    totalTokens: number;
}

function formatTokens(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return n.toLocaleString();
}

export function MetricsBar({ contextMetrics, totalTokens }: MetricsBarProps) {
    const densityDisplay = contextMetrics.filesExplored > 0
        ? `${Math.round(contextMetrics.contextDensity * 100)}%`
        : '\u2014';

    return (
        <div className="metrics-bar">
            <div className="metric">
                <span className="metric-label">FILES EXPLORED</span>
                <span className="metric-value">{contextMetrics.filesExplored}</span>
            </div>
            <div className="metric">
                <span className="metric-label">CONTEXT DENSITY</span>
                <span className="metric-value">{densityDisplay}</span>
            </div>
            <div className="metric">
                <span className="metric-label">DRILLDOWNS</span>
                <span className="metric-value">{contextMetrics.drilldownCount}</span>
            </div>
            <div className="metric">
                <span className="metric-label">API TOKENS</span>
                <span className="metric-value">{formatTokens(totalTokens)}</span>
            </div>
            <div className="metric">
                <span className="metric-label">SKELETON TOKENS</span>
                <span className="metric-value">{formatTokens(contextMetrics.skeletonTokens)}</span>
            </div>
        </div>
    );
}
