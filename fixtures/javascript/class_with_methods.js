import EventEmitter from 'events';

const DEFAULT_TIMEOUT = 5000;

/**
 * Manages WebSocket connections with auto-reconnect.
 */
export class ConnectionManager extends EventEmitter {
    /**
     * @param {string} url - The WebSocket URL.
     * @param {Object} options - Connection options.
     */
    constructor(url, options = {}) {
        super();
        this.url = url;
        this.timeout = options.timeout || DEFAULT_TIMEOUT;
        this._socket = null;
        this._retries = 0;
    }

    /**
     * Establish a connection to the server.
     * @returns {Promise<void>}
     */
    async connect() {
        this._socket = new WebSocket(this.url);
        this._socket.onopen = () => {
            this._retries = 0;
            this.emit('connected');
        };
        this._socket.onclose = () => {
            if (this._retries < 3) {
                this._retries++;
                setTimeout(() => this.connect(), this.timeout);
            }
        };
        this._socket.onerror = (err) => {
            this.emit('error', err);
        };
    }

    /**
     * Send a message through the connection.
     * @param {Object} data - The data to send.
     */
    send(data) {
        if (!this._socket) {
            throw new Error('Not connected');
        }
        const payload = JSON.stringify(data);
        this._socket.send(payload);
        this.emit('sent', data);
    }
}
