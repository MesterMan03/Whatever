import {Console, Engine, Entity, Message, Scene} from "@whatever-engine/api";

Message.registerMessageHandler((_) => {
    return "Hi there! :D";
});

Console.register({
    name: "test",
    description: "Eyy, we're testing :D",
    subcommands: [{
        name: "subcommand",
        description: "Subcommand test :3",
        args: [{
            name: "value",
            description: "Test value !!",
            required: true,
            type: "string"
        }],
        handler: (args) => {
            const value = args["value"] as string;
            return "You have entered: " + value;
        }
    }],
    handler: (_) => {
        return "Nope, try again with the subcommand <3";
    }
});

Console.register({
    name: "illegal",
    description: "Triggers a runtime crash to test how the engine responds",
    handler: (_) => {
        const illegal = "oopsies";
        // @ts-ignore
        return "You're not supposed to see this: " + illegal.lmao.noway;
    }
});

const previousSprites = new Set<Entity>();

Console.register({
    name: "showsprite",
    args: [{
        name: "texture",
        description: "The texture of the sprite",
        required: true,
        type: "string"
    }],
    handler: async (args) => {
        const texture = args["texture"] as string;
        const entity = await Scene.spawnSprite(texture, [0, 0, 0], [1, 1, 3]);
        for (const sprite of previousSprites) {
            const transform = await sprite.getComponent("core:transform");
            if (transform != null) {
                transform.addX(1.5).rotateY(15);
                sprite.setComponent("core:transform", transform);
            }
        }
        previousSprites.add(entity);
        return `Spawned sprite with entity id ${entity.id}`;
    }
});

Console.register({
    name: "testtick",
    description: "Tests the tick event",
    handler: (_) => {
        Engine.on("tick", async (ctx) => {
            const { tick_number, delta_seconds } = ctx;
            Engine.log("info", `Got tick even for #${tick_number}, delta ${delta_seconds} seconds`);
        });
        return "ok";
    }
});

Console.register({
    name: "testwatchdog",
    description: "Tests watchdog by creating a tick event handler that slows down the engine",
    handler: (_) => {
        Engine.on("tick", async (_) => {
            return new Promise((resolve) => {
                setTimeout(resolve, 2000);
            });
        });
        return "now we wait";
    }
});