interface ConnectionStatusProps {
    status: 'connecting' | 'open' | 'closed';
}

export function ConnectionStatus({ status }: ConnectionStatusProps) {
    if (status === 'open') return null;

    return (
        <div className="connection-status" role="status" aria-live="polite">
            {status === 'connecting'
                ? '// CONNECTING...'
                : '// DISCONNECTED \u2014 reconnecting...'}
        </div>
    );
}
