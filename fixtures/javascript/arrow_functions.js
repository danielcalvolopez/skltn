// Expression-bodied arrow functions (should NOT be pruned)
export const double = (x) => x * 2;
export const greet = (name) => `Hello, ${name}!`;
const identity = x => x;

// Block-bodied arrow functions (should be pruned)
export const processItems = (items) => {
    const results = [];
    for (const item of items) {
        if (item.active) {
            const transformed = {
                ...item,
                processedAt: Date.now(),
                label: item.label.toUpperCase(),
            };
            results.push(transformed);
        }
    }
    return results;
};

const fetchData = async (url) => {
    const response = await fetch(url);
    if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
    }
    const data = await response.json();
    return data;
};
