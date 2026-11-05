import { Component, Entity, Type, field } from "@whatever-engine/api";
import { position } from "./session.ts";
import type { Node } from "./navigation.ts";
export class NavNode extends Component {
  static readonly type = "core:NavNode";
  key = field(Type.String, "node");
  links = field(Type.Array(Type.String), []);
  enabled = true;
  spawnAllowed = true;
}
export function readGraph(root: Entity): Node[] {
  return root.getChildren().flatMap((entity) => {
    const node = entity.getComponent(NavNode);
    return node?.enabled ? [{ key: node.key, links: node.links, position: position(entity) }] : [];
  });
}
export function spawnNodes(root: Entity): Entity[] {
  return root.getChildren().filter((e) => {
    const node = e.getComponent(NavNode);
    return node?.enabled && node.spawnAllowed;
  });
}
