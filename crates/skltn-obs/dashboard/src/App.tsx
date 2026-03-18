import { useObsWebSocket } from './hooks/useObsWebSocket';
import { useSessionMetrics } from './hooks/useSessionMetrics';
import { useChartData } from './hooks/useChartData';
import { ConnectionStatus } from './components/ConnectionStatus';
import { ExplorationHero } from './components/ExplorationHero';
import { MetricsBar } from './components/MetricsBar';
import { TokenChart } from './components/TokenChart';
import { ModelBreakdown } from './components/ModelBreakdown';
import { RequestTable } from './components/RequestTable';

export default function App() {
    const { records, savingsRecords, drilldownRecords, status } = useObsWebSocket();
    const metrics = useSessionMetrics(records, savingsRecords, drilldownRecords);
    const chartOptions = useChartData(records, savingsRecords, metrics.contextMetrics.contextLimit);

    return (
        <div className="dashboard">
            <ConnectionStatus status={status} />
            <div className="dashboard-grid">
                <section className="dashboard-hero">
                    <ExplorationHero
                        multiplier={metrics.contextMetrics.explorationMultiplier}
                        hasSavings={savingsRecords.length > 0}
                    />
                </section>
                <header className="dashboard-header">
                    <MetricsBar
                        contextMetrics={metrics.contextMetrics}
                        totalTokens={metrics.totalTokens}
                    />
                </header>
                <main className="dashboard-main">
                    <TokenChart chartOptions={chartOptions} />
                </main>
                <aside className="dashboard-sidebar">
                    <ModelBreakdown modelBreakdown={metrics.modelBreakdown} />
                    <div className="cost-summary">
                        <div className="metric">
                            <span className="metric-label">SESSION COST</span>
                            <span className="metric-value cost">${metrics.totalCost.toFixed(2)}</span>
                        </div>
                        <div className="metric">
                            <span className="metric-label">COST SAVED</span>
                            <span className="metric-value cost">${metrics.skltnSavings.savingsUsd.toFixed(2)}</span>
                        </div>
                    </div>
                </aside>
                <footer className="dashboard-footer">
                    <RequestTable records={records} />
                </footer>
            </div>
        </div>
    );
}
