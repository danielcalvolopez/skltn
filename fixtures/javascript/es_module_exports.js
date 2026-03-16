export const API_VERSION = '2.0';
export const BASE_URL = 'https://api.example.com';

export function createClient(apiKey) {
    const headers = {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type': 'application/json',
        'X-API-Version': API_VERSION,
    };
    return {
        get: async (path) => {
            const res = await fetch(`${BASE_URL}${path}`, { headers });
            return res.json();
        },
        post: async (path, body) => {
            const res = await fetch(`${BASE_URL}${path}`, {
                method: 'POST',
                headers,
                body: JSON.stringify(body),
            });
            return res.json();
        },
    };
}

export default createClient;
