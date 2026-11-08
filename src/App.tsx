import { Sidebar } from "./components/Sidebar";
import { TabBar } from "./components/TabBar";
import { TerminalArea } from "./components/TerminalArea";
import { BrowserBanner } from "./components/BrowserBanner";

export default function App() {
  return (
    <>
      <div id="app">
        <Sidebar />
        <main id="main">
          <TabBar />
          <TerminalArea />
        </main>
      </div>
      <BrowserBanner />
    </>
  );
}