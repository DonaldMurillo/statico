import { Controller, Post, Body, UseGuards } from '@nestjs/common';
import { AuthService } from './auth.service';
import { JwtAuthGuard } from './guards/jwt-auth.guard';

@Controller('auth')
export class AuthController {
  constructor(private readonly authService: AuthService) {}

  @Post('login')
  login(@Body() body: { email: string; password: string }) {
    return this.authService.validateUser(body.email, body.password);
  }

  @Post('register')
  register(@Body() body: { name: string; email: string; password: string }) {
    return this.authService.register(body);
  }

  @Post('refresh')
  @UseGuards(JwtAuthGuard)
  refresh() {
    return { message: 'Token refreshed' };
  }
}
