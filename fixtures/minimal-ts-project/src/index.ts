import { add, multiply } from "./utils";
import _ from "lodash";

function main(): void {
	const result = add(1, 2);
	console.log(result);
	if (result > 0) {
		console.log("positive");
	}
}

main();
