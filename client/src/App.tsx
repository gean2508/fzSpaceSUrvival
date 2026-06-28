import { LandingBackdrop } from "./landing/LandingBackdrop";
import { LandingPage } from "./landing/LandingPage";
import { ParallaxDecor } from "./landing/ParallaxDecor";
import { useParallax } from "./landing/useParallax";
import { useScrollReveal } from "./landing/useScrollReveal";
import { useLandingAudio } from "./landing/useLandingAudio";

export default function App() {
  useParallax();
  useLandingAudio();
  const contentRef = useScrollReveal<HTMLDivElement>();

  return (
    <div className="landing">
      <LandingBackdrop />
      <ParallaxDecor />
      <div className="landing__overlay" ref={contentRef}>
        <LandingPage />
      </div>
    </div>
  );
}
