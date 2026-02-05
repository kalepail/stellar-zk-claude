/**
 * Xorshift32 - ZK-friendly seeded PRNG
 * Uses only 32-bit integer operations: XOR and bit shifts
 * Identical output in JS, Rust, and RISC-V
 */
export class SeededRng {
  private state: number;

  constructor(seed: number) {
    // Must be non-zero; use default if 0 provided
    this.state = (seed >>> 0) || 0xdeadbeef;
  }

  /** Get current seed state (for serialization) */
  getState(): number {
    return this.state;
  }

  /** Restore from saved state */
  setState(state: number): void {
    this.state = state >>> 0;
  }

  /** Generate next random u32 */
  next(): number {
    let x = this.state;
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    this.state = x >>> 0;
    return this.state;
  }

  /** Random integer in [0, max) */
  nextInt(max: number): number {
    return this.next() % max;
  }

  /** Random integer in [min, max) */
  nextRange(min: number, max: number): number {
    return min + this.nextInt(max - min);
  }

  /** Random float in [0, 1) - for compatibility during transition */
  nextFloat(): number {
    return this.next() / 0x100000000;
  }

  /** Random float in [min, max) - for compatibility during transition */
  nextFloatRange(min: number, max: number): number {
    return min + this.nextFloat() * (max - min);
  }

  /** Random boolean with given probability (0-1) */
  nextBool(probability: number = 0.5): boolean {
    return this.nextFloat() < probability;
  }
}
