import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { OnboardingCarousel } from "./OnboardingCarousel";

describe("OnboardingCarousel", () => {
  it("shows the first slide with Back disabled", () => {
    render(<OnboardingCarousel onDone={vi.fn()} />);

    expect(screen.getByText("Each bot has its own computer")).toBeVisible();
    expect(screen.getByRole("button", { name: "Back" })).toBeDisabled();
  });

  it("advances through slides and lets the user go back", async () => {
    const user = userEvent.setup();
    render(<OnboardingCarousel onDone={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(screen.getByText("Give each bot a job")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByText("Each bot has its own computer")).toBeVisible();
  });

  it("calls onDone once from the last slide", async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    render(<OnboardingCarousel onDone={onDone} />);

    for (let slide = 0; slide < 4; slide += 1) {
      await user.click(screen.getByRole("button", { name: "Next" }));
    }
    expect(
      screen.getByRole("button", { name: "Create your first bot" }),
    ).toBeVisible();
    await user.click(
      screen.getByRole("button", { name: "Create your first bot" }),
    );
    expect(onDone).toHaveBeenCalledOnce();
  });
});
