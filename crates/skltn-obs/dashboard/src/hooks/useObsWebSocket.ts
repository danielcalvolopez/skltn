import { useState, useEffect, useRef, useCallback } from 'react';
import type { UsageRecord } from '../types/usage';

type ConnectionStatus = 'connecting' | 'open' | 'closed';

interface UseObsWebSocketResult {
    records: UsageRecord[];
    status: ConnectionStatus;
}

export function useObsWebSocket(): UseObsWebSocketResult {
    const [records, setRecords] = useState<UsageRecord[]>([]);
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
        };

        ws.onmessage = (event) => {
            try {
                const record: UsageRecord = JSON.parse(event.data as string);
                setRecords((prev) => [...prev, record]);
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

    return { records, status };
}
