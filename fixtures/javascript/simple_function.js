import { readFileSync } from 'fs';
import path from 'path';

/**
 * Reads and parses a JSON configuration file.
 * @param {string} filePath - Path to the config file.
 * @returns {Object} The parsed configuration.
 */
export function readConfig(filePath) {
    const fullPath = path.resolve(filePath);
    const raw = readFileSync(fullPath, 'utf-8');
    const parsed = JSON.parse(raw);
    if (!parsed.name || !parsed.version) {
        throw new Error('Invalid config: missing name or version');
    }
    return parsed;
}

function validateEntry(entry) {
    if (typeof entry.id !== 'number') {
        throw new TypeError('Entry id must be a number');
    }
    if (!entry.label || entry.label.trim() === '') {
        throw new Error('Entry label is required');
    }
    return true;
}
