describe('App E2E', () => {
  it('should display the app', () => {
    cy.visit('/');
    cy.get('app-header').should('exist');
    cy.get('app-footer').should('exist');
  });
});
