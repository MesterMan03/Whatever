/** An entity reference embedded in component data. The wire representation is private. */
export interface EntityReference {
  readonly id: string;
  toJSON(): { $entity: string };
}

/** Explicitly mark an opaque ID as a reference, including inside custom components. */
export class EntityRef implements EntityReference {
  constructor(private readonly target: string | EntityReference) {}
  get id(): string {
    const id = this.toJSON().$entity;
    if (id.startsWith("pending:"))
      throw new Error(
        "Entity reference is pending; await its entity's get() before reading the ID",
      );
    return id;
  }
  toJSON(): { $entity: string } {
    return typeof this.target === "string" ? { $entity: this.target } : this.target.toJSON();
  }
}

export type ComponentValue =
  | string
  | number
  | boolean
  | null
  | EntityReference
  | ComponentValue[]
  | { [key: string]: ComponentValue };