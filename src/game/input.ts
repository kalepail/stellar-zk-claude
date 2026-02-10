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
  "KeyD", // Download tape
  "KeyL", // Load tape
  "Digit1", // Replay speed 1x
  "Digit2", // Replay speed 2x
  "Digit4", // Replay speed 4x
]);

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }

  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
    return true;
  }

  if (target instanceof HTMLSelectElement) {
    return true;
  }

  if (target instanceof HTMLElement && target.isContentEditable) {
    return true;
  }

  return false;
}

export class InputController {
  private readonly down = new Set<string>();
  private readonly pressed = new Set<string>();

  handleKeyDown(event: KeyboardEvent): void {
    if (!GAME_KEYS.has(event.code)) {
      return;
    }

    if (isEditableTarget(event.target)) {
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

    // Always release the key, even when focus is inside a form control,
    // so the game cannot get stuck in a "held key" state.
    if (isEditableTarget(event.target)) {
      this.down.delete(event.code);
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
