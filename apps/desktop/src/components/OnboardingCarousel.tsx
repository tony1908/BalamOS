import { useState } from "react";

type Slide = {
  title: string;
  body: string;
  motif: "computer" | "bots" | "chat" | "shield";
};

const slides: Slide[] = [
  {
    title: "Each bot has its own computer",
    body: "Every BalamOS bot runs in its own isolated Ubuntu workspace and drives it like you would.",
    motif: "computer",
  },
  {
    title: "Give each bot a job",
    body: "Create a bot per task — name it, point it at a project folder, and pick its harness and safety profile.",
    motif: "bots",
  },
  {
    title: "Chat to get it done",
    body: "Message a bot and it works in the background. Peek at its screen anytime, or expand it full-screen.",
    motif: "chat",
  },
  {
    title: "You stay in control",
    body: "Bots ask before risky steps — approve, decline, or interrupt. Start, stop, or reset any bot whenever you want.",
    motif: "shield",
  },
  {
    title: "Create your first bot",
    body: "Name a bot, point it at a project folder, and it gets to work. Let's set up your first one.",
    motif: "bots",
  },
];

function Illustration({ motif }: { motif: Slide["motif"] }) {
  if (motif === "bots") {
    return (
      <div className="onboarding-bots" aria-hidden="true">
        {[
          ["#db806e", "#f0a58d"],
          ["#6b9bd2", "#9dc4ed"],
          ["#c89b55", "#e4c17e"],
        ].map(([color, accent]) => (
          <span
            className="onboarding-avatar"
            style={{ background: color }}
            key={color}
          >
            <i style={{ background: accent }} />
            <i style={{ background: accent }} />
          </span>
        ))}
      </div>
    );
  }

  return (
    <svg
      className={`onboarding-illustration ${motif}`}
      viewBox="0 0 120 88"
      aria-hidden="true"
    >
      {motif === "computer" && (
        <>
          <rect x="15" y="14" width="90" height="56" rx="8" />
          <path d="M43 78h34M60 70v8" />
          <circle cx="60" cy="42" r="13" />
        </>
      )}
      {motif === "chat" && (
        <>
          <path d="M18 22a9 9 0 0 1 9-9h47a9 9 0 0 1 9 9v27a9 9 0 0 1-9 9H51L35 72V58H27a9 9 0 0 1-9-9Z" />
          <path d="M31 32h38M31 42h24" />
        </>
      )}
      {motif === "shield" && (
        <>
          <path d="m60 10 31 12v22c0 19-13 29-31 36-18-7-31-17-31-36V22Z" />
          <path d="m45 44 10 10 20-22" />
        </>
      )}
    </svg>
  );
}

export function OnboardingCarousel({ onDone }: { onDone: () => void }) {
  const [current, setCurrent] = useState(0);
  const slide = slides[current];
  const last = current === slides.length - 1;

  return (
    <div
      className="onboarding-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Welcome to BalamOS"
    >
      <div className="onboarding-card">
        {current === 0 && (
          <img
            className="onboarding-mascot"
            src="/balamos-guardian.png"
            alt="BalamOS guardian jaguar holding a protective shield"
          />
        )}
        <Illustration motif={slide.motif} />
        <p className="onboarding-kicker">BALAMOS / 0{current + 1}</p>
        <h1>{slide.title}</h1>
        <p className="onboarding-body">{slide.body}</p>
        <div
          className="onboarding-progress"
          aria-label={`Slide ${current + 1} of ${slides.length}`}
        >
          {slides.map((item, index) => (
            <span
              className={index === current ? "active" : ""}
              key={item.title}
            />
          ))}
        </div>
        <div className="onboarding-actions">
          <button
            className="onboarding-button primary"
            onClick={() => (last ? onDone() : setCurrent(current + 1))}
          >
            {last ? "Create your first bot" : "Next"}
          </button>
          <button
            className="onboarding-button secondary"
            disabled={current === 0}
            onClick={() => setCurrent(current - 1)}
          >
            Back
          </button>
        </div>
      </div>
    </div>
  );
}
