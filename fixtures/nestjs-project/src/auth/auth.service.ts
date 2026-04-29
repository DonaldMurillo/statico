import { Injectable } from '@nestjs/common';
import { JwtService } from '@nestjs/jwt';
import { LoggerService } from '../common/logger.service';

@Injectable()
export class AuthService {
  constructor(
    private jwtService: JwtService,
    private logger: LoggerService,
  ) {}

  async validateUser(email: string, password: string) {
    this.logger.log(`Validating user: ${email}`);
    const payload = { email, sub: '1' };
    return { access_token: this.jwtService.sign(payload) };
  }

  async register(body: { name: string; email: string; password: string }) {
    this.logger.log(`Registering user: ${body.email}`);
    const payload = { email: body.email, sub: '1' };
    return { access_token: this.jwtService.sign(payload) };
  }
}
