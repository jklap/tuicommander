import { Component, createEffect, createSignal } from "solid-js";
import type { SearchOptions } from "../shared/DomSearchEngine";
import type { CanvasTerminalRef } from "./CanvasTerminal";
import { SearchBar } from "../shared/SearchBar";

export interface TerminalSearchProps {
  visible: boolean;
  canvasRef?: CanvasTerminalRef | undefined;
  onClose: () => void;
}

export const TerminalSearch: Component<TerminalSearchProps> = (props) => {
  const [resultIndex, setResultIndex] = createSignal(-1);
  const [resultCount, setResultCount] = createSignal(0);

  let lastTerm = "";

  // Clear decorations when closing
  createEffect(() => {
    if (!props.visible) {
      props.canvasRef?.searchClear();
      setResultIndex(-1);
      setResultCount(0);
    }
  });

  const handleSearch = (term: string, _opts: SearchOptions) => {
    lastTerm = term;
    if (term) {
      props.canvasRef?.searchFind(term).then(({ index, count }) => {
        setResultIndex(index);
        setResultCount(count);
      });
    } else {
      props.canvasRef?.searchClear();
      setResultIndex(-1);
      setResultCount(0);
    }
  };

  const handleNext = () => {
    if (!lastTerm) return;
    const { index, count } = props.canvasRef?.searchNext() ?? { index: -1, count: 0 };
    setResultIndex(index);
    setResultCount(count);
  };

  const handlePrev = () => {
    if (!lastTerm) return;
    const { index, count } = props.canvasRef?.searchPrev() ?? { index: -1, count: 0 };
    setResultIndex(index);
    setResultCount(count);
  };

  return (
    <SearchBar
      visible={props.visible}
      onSearch={handleSearch}
      onNext={handleNext}
      onPrev={handlePrev}
      onClose={props.onClose}
      matchIndex={resultIndex()}
      matchCount={resultCount()}
    />
  );
};
