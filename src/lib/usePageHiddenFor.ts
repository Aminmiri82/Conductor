import { useEffect, useState } from "react";

// True once the app has been in the background (hidden, minimized, or behind
// Cmd-H) for `ms`, and false again as soon as it is visible. Heavy views use
// it to drop memory before WebKit's own cleanup of hidden pages, which runs a
// few minutes in and collects what nothing references any more. One timer
// runs per hide; nothing runs while visible.
export function usePageHiddenFor(ms: number) {
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const changed = () => {
      clearTimeout(timer);
      if (document.visibilityState === "hidden") {
        timer = setTimeout(() => setHidden(true), ms);
      } else {
        setHidden(false);
      }
    };
    changed();
    document.addEventListener("visibilitychange", changed);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", changed);
    };
  }, [ms]);

  return hidden;
}
