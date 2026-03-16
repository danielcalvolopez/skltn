function log(target, name, descriptor) {
    const original = descriptor.value;
    descriptor.value = function (...args) {
        console.log(`Calling ${name} with`, args);
        const result = original.apply(this, args);
        console.log(`${name} returned`, result);
        return result;
    };
    return descriptor;
}

class TaskManager {
    constructor() {
        this.tasks = [];
    }

    addTask(task) {
        this.tasks.push({
            ...task,
            createdAt: Date.now(),
            status: 'pending',
        });
        return this.tasks.length - 1;
    }

    completeTask(index) {
        if (index < 0 || index >= this.tasks.length) {
            throw new RangeError('Task index out of bounds');
        }
        this.tasks[index].status = 'completed';
        this.tasks[index].completedAt = Date.now();
    }
}

export { TaskManager };
