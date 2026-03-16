// Function overloads — signatures should be preserved, implementation pruned
export function format(value: string): string;
export function format(value: number): string;
export function format(value: Date): string;
export function format(value: string | number | Date): string {
    if (typeof value === 'string') {
        return value.trim();
    }
    if (typeof value === 'number') {
        return value.toFixed(2);
    }
    return value.toISOString();
}

export function parse(input: string, type: 'number'): number;
export function parse(input: string, type: 'boolean'): boolean;
export function parse(input: string, type: 'number' | 'boolean'): number | boolean {
    if (type === 'number') {
        const num = Number(input);
        if (isNaN(num)) throw new Error('Invalid number');
        return num;
    }
    return input === 'true';
}
