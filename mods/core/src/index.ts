import {Message} from "@whatever/api";

Message.registerMessageHandler((_) => {
    return "Hi there! :D";
});