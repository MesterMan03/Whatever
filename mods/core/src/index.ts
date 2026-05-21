import {Console, Message} from "@whatever/api";

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
            type: "int"
        }],
        handler: (args) => {
            const value = args["value"];
            if(value == null) {
                return "Aww, nothing :(";
            }
            return "You have entered: " + value;
        }
    }],
    handler: (args) => {
        return "Nope, try again with the subcommand <3";
    }
});