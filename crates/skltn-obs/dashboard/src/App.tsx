import { useObsWebSocket } from './hooks/useObsWebSocket';
import { useSessionMetrics } from './hooks/useSessionMetrics';
import { useChartData } from './hooks/useChartData';
import { ConnectionStatus } from './components/ConnectionStatus';
import { MetricsBar } from './components/MetricsBar';
import { TokenChart } from './components/TokenChart';
import { CacheRing } from './components/CacheRing';
import { ModelBreakdown } from './components/ModelBreakdown';
import { RequestTable } from './components/RequestTable';

export default function App() {
    const { records, status } = useObsWebSocket();
    const metrics = useSessionMetrics(records);
    const chartOptions = useChartData(records);

    return (
        <div className="dashboard">
            <ConnectionStatus status={status} />
            <div className="dashboard-grid">
                <header className="dashboard-header">
                    <MetricsBar
                        totalCost={metrics.totalCost}
                        cacheSavings={metrics.cacheSavings}
                        requestCount={metrics.requestCount}
                        totalTokens={metrics.totalTokens}
                    />
                </header>
                <main className="dashboard-main">
                    <TokenChart chartOptions={chartOptions} />
                </main>
                <aside className="dashboard-sidebar">
                    <CacheRing ratio={metrics.cacheHitRatio} />
                    <ModelBreakdown
                        modelBreakdown={metrics.modelBreakdown}
                    />
                </aside>
                <footer className="dashboard-footer">
                    <RequestTable records={records} />
                </footer>
            </div>
        </div>
    );
}
