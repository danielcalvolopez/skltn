interface MetricsBarProps {
    totalCost: number;
    cacheSavings: number;
    requestCount: number;
    totalTokens: number;
}

function formatTokens(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return n.toLocaleString();
}

export function MetricsBar({
    totalCost,
    cacheSavings,
    requestCount,
    totalTokens,
}: MetricsBarProps) {
    return (
        <div className="metrics-bar">
            <div className="metric">
                <span className="metric-label">SESSION COST</span>
                <span className="metric-value">${totalCost.toFixed(2)}</span>
            </div>
            <div className="metric">
                <span className="metric-label">CACHE SAVINGS</span>
                <span className="metric-value">${cacheSavings.toFixed(2)}</span>
            </div>
            <div className="metric">
                <span className="metric-label">REQUESTS</span>
                <span className="metric-value">
                    {requestCount.toLocaleString()}
                </span>
            </div>
            <div className="metric">
                <span className="metric-label">TOKENS</span>
                <span className="metric-value">
                    {formatTokens(totalTokens)}
                </span>
            </div>
        </div>
    );
}
