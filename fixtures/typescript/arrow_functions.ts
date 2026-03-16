import { User, Result } from './types';

// Expression-bodied (should NOT be pruned)
export const isAdmin = (user: User): boolean => user.role === 'admin';
export const getUserName = (user: User): string => user.name;

// Block-bodied (should be pruned)
export const validateUser = (user: User): Result<User> => {
    if (!user.name || user.name.trim() === '') {
        return { ok: false, error: new Error('Name is required') };
    }
    if (!user.email || !user.email.includes('@')) {
        return { ok: false, error: new Error('Valid email is required') };
    }
    return { ok: true, value: user };
};

export const fetchUsers = async (page: number = 1): Promise<User[]> => {
    const response = await fetch(`/api/users?page=${page}`);
    if (!response.ok) {
        throw new Error(`Failed to fetch users: ${response.status}`);
    }
    const data = await response.json();
    return data.users;
};
