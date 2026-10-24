/** Shown under the Overview greeting, one picked at random per visit. */
export const quotes: { text: string; author: string }[] = [
  { text: "Simplicity is prerequisite for reliability.", author: "Edsger W. Dijkstra" },
  { text: "Talk is cheap. Show me the code.", author: "Linus Torvalds" },
  {
    text: "Programs must be written for people to read, and only incidentally for machines to execute.",
    author: "Harold Abelson",
  },
  { text: "Premature optimization is the root of all evil.", author: "Donald Knuth" },
  { text: "Make it work, make it right, make it fast.", author: "Kent Beck" },
  {
    text: "Any fool can write code that a computer can understand. Good programmers write code that humans can understand.",
    author: "Martin Fowler",
  },
  {
    text: "There are only two hard things in Computer Science: cache invalidation and naming things.",
    author: "Phil Karlton",
  },
  { text: "The best way to predict the future is to invent it.", author: "Alan Kay" },
  { text: "The most dangerous phrase in the language is: we've always done it this way.", author: "Grace Hopper" },
  { text: "Controlling complexity is the essence of computer programming.", author: "Brian Kernighan" },
  {
    text: "Debugging is twice as hard as writing the code in the first place.",
    author: "Brian Kernighan",
  },
  { text: "Testing shows the presence, not the absence of bugs.", author: "Edsger W. Dijkstra" },
  { text: "Deleted code is debugged code.", author: "Jeff Sickel" },
  { text: "Before software can be reusable it first has to be usable.", author: "Ralph Johnson" },
  {
    text: "Perfection is achieved not when there is nothing more to add, but when there is nothing left to take away.",
    author: "Antoine de Saint-Exupéry",
  },
  {
    text: "Measuring programming progress by lines of code is like measuring aircraft building progress by weight.",
    author: "Bill Gates",
  },
  { text: "Code is like humor. When you have to explain it, it's bad.", author: "Cory House" },
  { text: "Real artists ship.", author: "Steve Jobs" },
];

export const randomQuote = () => quotes[Math.floor(Math.random() * quotes.length)];
