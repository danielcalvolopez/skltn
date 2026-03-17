import type { ModelCost } from '../types/usage';

interface ModelBreakdownProps {
    modelBreakdown: ModelCost[];
}

export function ModelBreakdown({ modelBreakdown }: ModelBreakdownProps) {
    const maxCost = modelBreakdown.length > 0 ? modelBreakdown[0]!.cost : 0;

    return (
        <div className="model-breakdown">
            <span className="metric-label">MODEL BREAKDOWN</span>
            <ul className="model-list">
                {modelBreakdown.map(({ model, cost }) => (
                    <li key={model} className="model-item">
                        <span className="model-name">{model}</span>
                        <span className="model-cost">
                            ${cost.toFixed(2)}
                        </span>
                        <div className="model-bar-track">
                            <div
                                className="model-bar-fill"
                                style={{
                                    width:
                                        maxCost > 0
                                            ? `${(cost / maxCost) * 100}%`
                                            : '0%',
                                }}
                            />
                        </div>
                    </li>
                ))}
            </ul>
        </div>
    );
}
