import { useState, useEffect, useRef, useCallback } from 'react';
import type { UsageRecord, SavingsRecord, DrilldownRecord } from '../types/usage';

type ConnectionStatus = 'connecting' | 'open' | 'closed';

interface UseObsWebSocketResult {
    records: UsageRecord[];
    savingsRecords: SavingsRecord[];
    drilldownRecords: DrilldownRecord[];
    status: ConnectionStatus;
}

interface TypedMessage {
    type: 'usage' | 'savings' | 'drilldown';
    data: UsageRecord | SavingsRecord | DrilldownRecord;
}

function isTypedMessage(msg: unknown): msg is TypedMessage {
    return (
        typeof msg === 'object' &&
        msg !== null &&
        'type' in msg &&
        'data' in msg &&
        ((msg as TypedMessage).type === 'usage' || (msg as TypedMessage).type === 'savings' || (msg as TypedMessage).type === 'drilldown')
    );
}

export function useObsWebSocket(): UseObsWebSocketResult {
    const [records, setRecords] = useState<UsageRecord[]>([]);
    const [savingsRecords, setSavingsRecords] = useState<SavingsRecord[]>([]);
    const [drilldownRecords, setDrilldownRecords] = useState<DrilldownRecord[]>([]);
    const [status, setStatus] = useState<ConnectionStatus>('connecting');
    const backoffRef = useRef(1000);
    const wsRef = useRef<WebSocket | null>(null);
    const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
        null,
    );

    const connect = useCallback(() => {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const url = `${protocol}//${window.location.host}/ws`;

        setStatus('connecting');
        const ws = new WebSocket(url);
        wsRef.current = ws;

        ws.onopen = () => {
            setStatus('open');
            backoffRef.current = 1000;
            // Clear records — replay from server is authoritative
            setRecords([]);
            setSavingsRecords([]);
            setDrilldownRecords([]);
        };

        ws.onmessage = (event) => {
            try {
                const parsed = JSON.parse(event.data as string);

                // Support typed envelope format (future) and raw UsageRecord (current)
                if (isTypedMessage(parsed)) {
                    if (parsed.type === 'usage') {
                        setRecords((prev) => [...prev, parsed.data as UsageRecord]);
                    } else if (parsed.type === 'savings') {
                        setSavingsRecords((prev) => [...prev, parsed.data as SavingsRecord]);
                    } else if (parsed.type === 'drilldown') {
                        setDrilldownRecords((prev) => [...prev, parsed.data as DrilldownRecord]);
                    }
                } else {
                    // Legacy: raw UsageRecord without envelope
                    setRecords((prev) => [...prev, parsed as UsageRecord]);
                }
            } catch {
                // Ignore malformed messages
            }
        };

        ws.onclose = () => {
            setStatus('closed');
            wsRef.current = null;
            // Schedule reconnect with exponential backoff
            const delay = backoffRef.current;
            backoffRef.current = Math.min(delay * 2, 30000);
            reconnectTimerRef.current = setTimeout(connect, delay);
        };

        ws.onerror = () => {
            // onclose will fire after onerror — reconnect handled there
        };
    }, []);

    useEffect(() => {
        connect();

        return () => {
            if (reconnectTimerRef.current) {
                clearTimeout(reconnectTimerRef.current);
            }
            if (wsRef.current) {
                wsRef.current.close();
            }
        };
    }, [connect]);

    return { records, savingsRecords, drilldownRecords, status };
}
