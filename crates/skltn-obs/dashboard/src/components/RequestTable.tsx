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
                        <th>CACHE W</th>
                        <th>CACHE R</th>
                        <th>COST</th>
                    </tr>
                </thead>
                <tbody>
                    {reversed.map((record, i) => (
                        <tr key={`${record.timestamp}-${i}`}>
                            <td>{formatTimestamp(record.timestamp)}</td>
                            <td>{shortModel(record.model)}</td>
                            <td>{record.input_tokens.toLocaleString()}</td>
                            <td>{record.output_tokens.toLocaleString()}</td>
                            <td>
                                {record.cache_creation_input_tokens.toLocaleString()}
                            </td>
                            <td>
                                {record.cache_read_input_tokens.toLocaleString()}
                            </td>
                            <td>${record.cost_usd.toFixed(4)}</td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
}
