import type { UsageRecord } from '../types/usage';

interface RequestTableProps {
    records: UsageRecord[];
}

function formatTimestamp(ts: string): string {
    const d = new Date(ts);
    return d.toLocaleTimeString('en-US', { hour12: false });
}

function shortModel(model: string): string {
    return model
        .replace('claude-', '')
        .replace(/-\d{8}$/, '');
}

function formatTokens(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return n.toLocaleString();
}

export function RequestTable({ records }: RequestTableProps) {
    const reversed = [...records].reverse();

    return (
        <div className="request-table">
            <table>
                <thead>
                    <tr>
                        <th>TIME</th>
                        <th>MODEL</th>
                        <th>INPUT</th>
                        <th>OUTPUT</th>
                        <th>TOTAL</th>
                        <th>COST</th>
                    </tr>
                </thead>
                <tbody>
                    {reversed.map((record, i) => {
                        const total = record.input_tokens + record.output_tokens +
                            record.cache_read_input_tokens + record.cache_creation_input_tokens;
                        return (
                            <tr key={`${record.timestamp}-${i}`}>
                                <td>{formatTimestamp(record.timestamp)}</td>
                                <td>{shortModel(record.model)}</td>
                                <td>{formatTokens(record.input_tokens)}</td>
                                <td>{formatTokens(record.output_tokens)}</td>
                                <td>{formatTokens(total)}</td>
                                <td>${record.cost_usd.toFixed(4)}</td>
                            </tr>
                        );
                    })}
                </tbody>
            </table>
        </div>
    );
}
