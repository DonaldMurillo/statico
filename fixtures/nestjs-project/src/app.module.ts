import { Module } from '@nestjs/common';
import { ConfigModule } from './config/config.module';
import { AppController } from './app.controller';
import { AppService } from './app.service';
import { UsersModule } from './users/users.module';
import { AuthModule } from './auth/auth.module';
import { HealthModule } from './health/health.module';

@Module({
  imports: [ConfigModule, UsersModule, AuthModule, HealthModule],
  controllers: [AppController],
  providers: [AppService],
})
export class AppModule {}
