export abstract class BaseService<T> {
    protected cache: Map<number, T> = new Map();

    abstract findById(id: number): Promise<T | null>;
    abstract create(data: Omit<T, 'id'>): Promise<T>;

    async findCached(id: number): Promise<T | null> {
        if (this.cache.has(id)) {
            return this.cache.get(id)!;
        }
        const result = await this.findById(id);
        if (result) {
            this.cache.set(id, result);
        }
        return result;
    }

    clearCache(): void {
        this.cache.clear();
        console.log('Cache cleared');
    }
}

export class UserServiceImpl extends BaseService<User> {
    constructor(private readonly db: Database) {
        super();
    }

    async findById(id: number): Promise<User | null> {
        const row = await this.db.query('SELECT * FROM users WHERE id = $1', [id]);
        if (!row) return null;
        return this.mapRow(row);
    }

    async create(data: Omit<User, 'id'>): Promise<User> {
        const row = await this.db.query(
            'INSERT INTO users (name, email, role) VALUES ($1, $2, $3) RETURNING *',
            [data.name, data.email, data.role]
        );
        return this.mapRow(row);
    }

    private mapRow(row: any): User {
        return {
            id: row.id,
            name: row.name,
            email: row.email,
            role: row.role,
        };
    }
}
