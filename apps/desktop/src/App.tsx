import { useGameStore } from "./store/useGameStore";
import { Game } from "./ui/Game";
import { SplashScreen } from "./ui/screens/SplashScreen";
import { MainMenu } from "./ui/screens/MainMenu";
import { ModeSelect } from "./ui/screens/ModeSelect";
import { ClassSelect } from "./ui/screens/ClassSelect";
import { AuthScreen } from "./ui/screens/AuthScreen";
import { SettingsScreen } from "./ui/screens/SettingsScreen";
import { ResultsScreen } from "./ui/screens/ResultsScreen";

export default function App() {
  const screen = useGameStore((s) => s.screen);

  switch (screen) {
    case "splash":
      return <SplashScreen />;
    case "menu":
      return <MainMenu />;
    case "mode":
      return <ModeSelect />;
    case "class":
      return <ClassSelect />;
    case "auth":
      return <AuthScreen />;
    case "settings":
      return <SettingsScreen />;
    case "results":
      return <ResultsScreen />;
    case "game":
      return <Game />;
    default:
      return <MainMenu />;
  }
}
