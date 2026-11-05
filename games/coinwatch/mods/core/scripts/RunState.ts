import { Component } from "@whatever-engine/api";
/** Transient run data, stored in ECS so all systems read the same scene revision. */
export class RunState extends Component {
  static readonly type = "core:RunState";
  score = 0;
  coins = 0;
  kills = 0;
  over = false;
  elapsed = 0;
  generation = 0;
  readyAfter = -1;
}
