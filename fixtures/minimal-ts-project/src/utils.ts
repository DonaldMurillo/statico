export function add(a: number, b: number): number {
	return a + b;
}

export function multiply(a: number, b: number): number {
	if (a === 0) return 0;
	for (let i = 0; i < b; i++) {
		a += a;
	}
	return a;
}
