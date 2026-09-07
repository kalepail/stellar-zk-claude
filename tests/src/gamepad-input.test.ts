import { afterEach, describe, expect, it } from "bun:test";
import { InputController } from "../../src/game/input";

const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, "navigator");

type MockGamepad = {
  connected: boolean;
  buttons: Array<{ pressed: boolean }>;
  axes: number[];
};

function buttonState(pressed: number[] = []): Array<{ pressed: boolean }> {
  return Array.from({ length: 16 }, (_, index) => ({ pressed: pressed.includes(index) }));
}

function installGamepads(gamepads: Array<MockGamepad | null>): void {
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: { getGamepads: () => gamepads },
  });
}

afterEach(() => {
  if (originalNavigator) {
    Object.defineProperty(globalThis, "navigator", originalNavigator);
  } else {
    Reflect.deleteProperty(globalThis, "navigator");
  }
});

describe("InputController gamepad polling", () => {
  it("maps left-stick movement only beyond the deadzone", () => {
    const input = new InputController();

    installGamepads([{ connected: true, buttons: buttonState(), axes: [-0.49, 0] }]);
    input.pollGamepad();
    expect(input.isDown("ArrowLeft")).toBe(false);

    installGamepads([{ connected: true, buttons: buttonState(), axes: [-0.51, 0] }]);
    input.pollGamepad();
    expect(input.isDown("ArrowLeft")).toBe(true);
  });

  it("edge-triggers gamepad presses instead of repeating every poll", () => {
    const input = new InputController();
    const pad = { connected: true, buttons: buttonState([7]), axes: [0, 0] };

    installGamepads([pad]);
    input.pollGamepad();
    expect(input.isDown("Space")).toBe(true);
    expect(input.consumePress("Space")).toBe(true);

    input.pollGamepad();
    expect(input.consumePress("Space")).toBe(false);
  });

  it("does not clear keyboard state when the gamepad releases the same action", () => {
    const input = new InputController();
    const keyboardEvent = {
      code: "ArrowLeft",
      target: null,
      preventDefault: () => {},
    } as unknown as KeyboardEvent;

    input.handleKeyDown(keyboardEvent);
    installGamepads([{ connected: true, buttons: buttonState(), axes: [-1, 0] }]);
    input.pollGamepad();
    expect(input.isDown("ArrowLeft")).toBe(true);

    installGamepads([]);
    input.pollGamepad();
    expect(input.isDown("ArrowLeft")).toBe(true);

    input.handleKeyUp(keyboardEvent);
    expect(input.isDown("ArrowLeft")).toBe(false);
  });
});
