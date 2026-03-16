import { readFileSync } from 'fs';
import path from 'path';

interface Config {
    name: string;
    version: string;
    entries: ConfigEntry[];
}

interface ConfigEntry {
    id: number;
    label: string;
    enabled: boolean;
}

type ConfigValidator = (config: Config) => boolean;

export function readConfig(filePath: string): Config {
    const fullPath = path.resolve(filePath);
    const raw = readFileSync(fullPath, 'utf-8');
    const parsed: Config = JSON.parse(raw);
    if (!validateConfig(parsed)) {
        throw new Error('Invalid configuration');
    }
    return parsed;
}

function validateConfig(config: Config): boolean {
    if (!config.name || !config.version) {
        return false;
    }
    if (!Array.isArray(config.entries)) {
        return false;
    }
    return config.entries.every(e => typeof e.id === 'number');
}
