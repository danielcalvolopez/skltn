function Injectable() {
    return function (target: any) {
        Reflect.defineMetadata('injectable', true, target);
    };
}

function Log(target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const original = descriptor.value;
    descriptor.value = function (...args: any[]) {
        console.log(`[${propertyKey}] called with:`, args);
        const result = original.apply(this, args);
        console.log(`[${propertyKey}] returned:`, result);
        return result;
    };
    return descriptor;
}

@Injectable()
export class OrderService {
    private orders: Map<string, Order> = new Map();

    @Log
    createOrder(items: OrderItem[]): Order {
        const order: Order = {
            id: crypto.randomUUID(),
            items,
            total: items.reduce((sum, item) => sum + item.price * item.quantity, 0),
            createdAt: new Date(),
        };
        this.orders.set(order.id, order);
        return order;
    }

    @Log
    cancelOrder(orderId: string): boolean {
        const order = this.orders.get(orderId);
        if (!order) return false;
        this.orders.delete(orderId);
        return true;
    }
}
