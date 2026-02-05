const GAME_KEYS = new Set([
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "Space",
  "KeyP",
  "Enter",
  "KeyR",
  "KeyA", // Autopilot toggle
  "Escape", // Return to menu
]);

export class InputController {
  private readonly down = new Set<string>();
  private readonly pressed = new Set<string>();

  handleKeyDown(event: KeyboardEvent): void {
    if (!GAME_KEYS.has(event.code)) {
      return;
    }

    event.preventDefault();

    if (!this.down.has(event.code)) {
      this.pressed.add(event.code);
    }

    this.down.add(event.code);
  }

  handleKeyUp(event: KeyboardEvent): void {
    if (!GAME_KEYS.has(event.code)) {
      return;
    }

    event.preventDefault();
    this.down.delete(event.code);
  }

  isDown(code: string): boolean {
    return this.down.has(code);
  }

  consumePress(code: string): boolean {
    const wasPressed = this.pressed.has(code);
    this.pressed.delete(code);
    return wasPressed;
  }

  clearPressed(): void {
    this.pressed.clear();
  }

  reset(): void {
    this.down.clear();
    this.pressed.clear();
  }
}
