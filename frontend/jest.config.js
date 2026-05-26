/** @type {import('jest').Config} */
module.exports = {
  testEnvironment: "jest-environment-jsdom",
  transform: { "^.+\\.(ts|tsx|js|jsx)$": "babel-jest" },
  moduleNameMapper: {
    "^../styles/(.*)$": "<rootDir>/__mocks__/styleMock.js",
    "^../../styles/(.*)$": "<rootDir>/__mocks__/styleMock.js",
    "^../lib/api$": "<rootDir>/__mocks__/api.ts",
    "^../../lib/api$": "<rootDir>/__mocks__/api.ts",
    "^../lib/carbon-utils$": "<rootDir>/__mocks__/carbon-utils.ts",
    "^../../lib/carbon-utils$": "<rootDir>/__mocks__/carbon-utils.ts",
  },
  setupFilesAfterEnv: ["@testing-library/jest-dom"],
};
