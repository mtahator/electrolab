// Back/forward history of places in a tab (P1.M2.VW.6). Every navigation (link, outline,
// search hit, bookmark, page box) pushes where the reader was, so Back always returns there.

export interface Place {
  page: number;
  /** Page-space y at the top of the view. */
  y: number;
}

export class History {
  private back: Place[] = [];
  private forward: Place[] = [];

  /** Record `from` before jumping somewhere else. */
  push(from: Place): void {
    const last = this.back[this.back.length - 1];
    if (last && last.page === from.page && Math.abs(last.y - from.y) < 4) return;
    this.back.push(from);
    if (this.back.length > 200) this.back.shift();
    this.forward = [];
  }

  /** Where Back goes from `here`, or null. */
  goBack(here: Place): Place | null {
    const p = this.back.pop();
    if (!p) return null;
    this.forward.push(here);
    return p;
  }

  /** Where Forward goes from `here`, or null. */
  goForward(here: Place): Place | null {
    const p = this.forward.pop();
    if (!p) return null;
    this.back.push(here);
    return p;
  }

  canBack(): boolean {
    return this.back.length > 0;
  }

  canForward(): boolean {
    return this.forward.length > 0;
  }
}
