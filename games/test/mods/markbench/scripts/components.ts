import { Component, Type, field } from "@whatever-engine/api";
export class Payload extends Component {
  static readonly type = "markbench:payload";
  value = field(Type.F64, 0);
  cold = field(
    Type.Array(Type.F64, { maxLength: 64 }),
    [
      0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
      26, 27, 28, 29, 30, 31,
    ],
  );
}
export class Counter extends Component {
  static readonly type = "markbench:counter";
  value = 0;
  override tick(): void {
    this.value++;
  }
}
