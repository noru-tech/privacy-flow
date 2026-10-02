export interface Analyzer {
  id: string;
  analyze(value: string): void;
}

class AnalyzerRegistry {
  private readonly analyzers: Map<string, Analyzer> = new Map();

  register(analyzer: Analyzer) {
    this.analyzers.set(analyzer.id, analyzer);
  }

  get(id: string) {
    return this.analyzers.get(id);
  }

  each(scheduler: Scheduler) {
    for (const [, analyzer] of this.analyzers) {
      scheduler.add(analyzer);
    }
  }
}

export class Scheduler {
  private readonly queue = new Map<string, Analyzer>();

  add(analyzer: Analyzer) {
    this.queue.set(analyzer.id, analyzer);
  }

  run(id: string, value: string) {
    for (const analyzer of this.queue.values()) {
      if (analyzer.id === id) {
        analyzer.analyze(value);
      }
    }
  }
}

export const registry = new AnalyzerRegistry();
